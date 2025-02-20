use std::io::Error as IoError;
use std::mem;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

use futures::future::Future;
use futures::Sink;
use futures::SinkExt;
use log::debug;
use log::trace;
use log::error;
use pin_utils::pin_mut;
use pin_utils::unsafe_pinned;
use pin_utils::unsafe_unpinned;

use future_aio::fs::create_dir_all;
use kf_protocol::api::ErrorCode;
use kf_protocol::api::DefaultBatch;
use kf_protocol::api::Offset;
use kf_protocol::api::Size;
use kf_protocol::api::DefaultRecords;
use kf_protocol::api::Isolation;


use crate::checkpoint::CheckPoint;
use crate::range_map::SegmentList;
use crate::segment::MutableSegment;
use crate::ConfigOption;
use crate::SegmentSlice;
use crate::StorageError;
use crate::SlicePartitionResponse;
use crate::ReplicaStorage;

/// alway evaluate expression and return
/// this is usesful for debugging value
macro_rules! f_trace {
    ($target:expr,$value:expr) => {{
        let res = $value;
        trace!($target, res);
        res
    }};
}

// Based on Sink Buffer:
// it accumulates item into holding state until we clear either segment creation or active sgement
#[derive(Debug)]
enum ReplicateState {
    Active,
    Rollover(DefaultBatch, Offset),
}

impl ReplicateState {
    fn roll_over_item(self) -> DefaultBatch {
        match self {
            ReplicateState::Rollover(item, _) => item,
            _ => panic!("should only be called for rollover state"),
        }
    }
}

/// Sink for Replica.  Replica is lowest public API for commit log
/// Internally it is storeed as list of segments.  Each segment contains finite sets of record batches.
///
#[derive(Debug)]
pub struct FileReplica {
    #[allow(dead_code)]
    last_base_offset: Offset,
    #[allow(dead_code)]
    partition: Size,
    option: ConfigOption,
    active_segment: MutableSegment,
    state: ReplicateState,
    prev_segments: SegmentList,
    commit_checkpoint: CheckPoint<Offset>,
}

impl Unpin for FileReplica {}

impl ReplicaStorage for FileReplica {
    

    fn get_hw(&self) -> Offset {
        *self.commit_checkpoint.get_offset()
    }


    /// offset mark that beggining of uncommitted
    fn get_leo(&self) -> Offset {
        self.active_segment.get_end_offset()
    }

}

impl FileReplica {
    unsafe_pinned!(active_segment: MutableSegment);
    unsafe_unpinned!(prev_segments: SegmentList);

    /// Construct a new replica with specified topic and partition.
    /// It can start with arbitrary offset.  However, for normal replica,
    /// it is usually starts with 0.  
    ///
    /// Replica is minimum unit of logs that will can be replicated.  
    /// It is a unique pair of (topic,partiton)
    ///
    /// Replica will use base directory to create it's own directory.
    /// Directory name will encode unique replica id which is combination of topic
    /// and partition.
    ///
    /// If there is existing directory then it will load existing logs.
    /// The logs will be validated to ensure it's safe to use it.
    /// It is possible logs can't be used because they may be corrupted.
    pub async fn create<S>(
        topic: S,
        partition: Size,
        base_offset: Offset,
        option: &ConfigOption,
    ) -> Result<FileReplica, StorageError>
    where
        S: AsRef<str> + Send + 'static,
    {
        let replica_dir = option.base_dir.join(replica_dir_name(topic, partition));

        debug!("creating rep dir: {}", replica_dir.display());
        create_dir_all(&replica_dir).await?; // ensure dir_name exits

        let mut rep_option = option.clone();
        rep_option.base_dir = replica_dir;
        // create acive segment

        let (segments, last_offset_res) = SegmentList::from_dir(&rep_option).await?;

        let active_segment = if let Some(last_offset) = last_offset_res {
            trace!("last segment found, validating offsets: {}", last_offset);
            let mut last_segment =
                MutableSegment::open_for_write(last_offset, &rep_option).await?;
            last_segment.validate().await?;
            trace!(
                "segment validated with last offset: {}",
                last_segment.get_end_offset()
            );
            last_segment
        } else {
            debug!("no segment found, creating new one");
            MutableSegment::create(base_offset, &rep_option).await?
        };

        let last_base_offset = active_segment.get_base_offset();

        let commit_checkpoint: CheckPoint<Offset> = CheckPoint::create(
            &rep_option,
            "replication.chk",
            last_base_offset
        ).await?;

        Ok(FileReplica {
            option: rep_option,
            last_base_offset,
            partition,
            active_segment,
            state: ReplicateState::Active,
            prev_segments: segments,
            commit_checkpoint,
        })
    }

    /// update committed offset (highwatermark)
    pub async fn update_high_watermark(&mut self, offset: Offset) -> Result<(), IoError> {
        let old_offset = self.get_hw();
        if old_offset == offset {
            trace!("new high watermark: {} is same as existing one, skipping",offset);
            Ok(())
        } else {
            trace!("updating to new highwatermark: {} old: {}",old_offset,offset);
            self.commit_checkpoint.write(offset).await
        }       
    }

     /// update high watermark to 
    pub async fn update_high_watermark_to_end(&mut self) -> Result<(),IoError>{
        
        self.update_high_watermark(self.get_leo()).await
    }


    /// earliest offset
    pub fn get_log_start_offset(&self) -> Offset {
        let min_base_offset = self.prev_segments.min_offset();
        if min_base_offset < 0 {
            self.active_segment.get_base_offset()
        } else {
            min_base_offset
        }
    }

    /// find the segment that contains offsets
    /// segment could be active segment which can be written
    /// or read only segment.
    pub(crate) fn find_segment(&self, offset: Offset) -> Option<SegmentSlice> {

        trace!("finding segment for: {}",offset);
        if offset >= self.active_segment.get_base_offset()  {
            trace!("active segment found for: {}", offset);
            Some(self.active_segment.to_segment_slice())
        } else {
            trace!("offset is before active, searching prev segment");
            self.prev_segments
                .find_segment(offset)
                .map(|(_, segment)| segment.to_segment_slice())
        }
    }

    /// write records to this replica, update highwatermark if required
    pub async fn send_records(&mut self, records: DefaultRecords, update_highwatermark: bool) -> Result<(),StorageError>{
        
        for batch in records.batches {
           self.send(batch).await?;
        }

        if update_highwatermark {
            self.update_high_watermark_to_end().await?;
        }

        Ok(())
    }


    /// read uncommitted records( between highwatermark and end offset) to file response
    pub async fn read_uncommitted_records<P>(&self, response: &mut P)  where P: SlicePartitionResponse{
        self.read_records(self.get_hw(),None,response).await   
    }

    /// read committed records
    pub async fn read_committed_records<P>(&self, start_offset: Offset, response: &mut P) where P: SlicePartitionResponse {
        self.read_records(start_offset,Some(self.get_hw()),response).await
    }


    pub async fn read_records_with_isolation<P>(
        &self,
        offset: Offset,
        isolation: Isolation,
        partition_response: &mut P,
        ) where
            P: SlicePartitionResponse,
    {
            match isolation {
                Isolation::ReadCommitted => {
                
                    self
                        .read_committed_records(offset, partition_response)
                        .await
                }
                Isolation::ReadUncommitted => {
                    self
                        .read_records(offset, None, partition_response)
                        .await
                }
            }
    }

    /// read records
    /// * `start_offset`:  start offsets
    /// * `max_offset`:  max offset (exclusive)
    /// * `responsive`:  output
    pub async fn read_records<P>(&self,start_offset: Offset,max_offset: Option<Offset>,response: &mut P)   where P: SlicePartitionResponse {
        
        trace!("read records to response from: {} max: {:#?}",start_offset,max_offset);

        let highwatermark = self.get_hw();
        response.set_hw(highwatermark);
        response.set_last_stable_offset(highwatermark);
        response.set_log_start_offset(self.get_log_start_offset());

        match self.find_segment(start_offset) {
            Some(segment) => {

                let slice = 
                    match segment {
                        SegmentSlice::MutableSegment(segment) => {
                            // optimization
                            if start_offset == self.get_leo() {
                                trace!("start offset is same as end offset, skipping");
                                return
                            } else {
                                debug!("active segment with baseoffset: {} found for offset: {}",segment.get_base_offset(),start_offset);
                                segment.records_slice(start_offset,max_offset).await
                            }
                           
                        },
                        SegmentSlice::Segment(segment) => {
                            debug!("read segment with baseoffset: {} found for offset: {}",segment.get_base_offset(),start_offset);
                            segment.records_slice(start_offset,max_offset).await
                        }
                    };
                
                
                match slice {

                    Ok(slice) => {
                        match slice {
                            Some(slice) => {
                                debug!("retrieved slice: {:#?}",slice);
                                response.set_slice(slice);
                            },
                            None => {
                                debug!("records not found for: {}",start_offset);
                                response.set_error_code(ErrorCode::OffsetOutOfRange);

                            }
                        }
                    },
                    Err(err) => {
                        response.set_error_code(ErrorCode::UnknownServerError);
                        error!("error fetch: {:#?}",err);
                    }
                }
                
            },
            None => {
                response.set_error_code(ErrorCode::OffsetOutOfRange);
                debug!("segment not found for offset: {}",start_offset);
            }
        }
    }


}

impl Sink<DefaultBatch> for FileReplica {
  
    type Error = StorageError;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        debug!("start polling ready");
        match self.as_ref().state {
            ReplicateState::Active => {
                trace!("in active state. polling segment for ready status");
                f_trace!(
                    "segment status: {:#?}",
                    self.active_segment().poll_ready(cx)
                )
            }
            _ => panic!("should not poll when in roll over state"),
        }
    }

    fn start_send(mut self: Pin<&mut Self>, item: DefaultBatch) -> Result<(), Self::Error> {
        debug!("start_send");
        match self.as_ref().state {
            ReplicateState::Active => {
                let active_segment = &mut self.as_mut().active_segment;
                let offset = active_segment.get_end_offset();
                match self.as_mut().active_segment().start_send(item) {
                    Err(err) => match err {
                        StorageError::NoRoom(item) => {
                            mem::replace(&mut self.state, ReplicateState::Rollover(item, offset));
                            Ok(())
                        }
                        _ => Err(err),
                    },
                    _ => Ok(()),
                }
            }
            _ => panic!("do not send start to roll over"),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        debug!("flushing");
        // get unpinned to self, you can't do multiple mut self
        match self.as_ref().state {
            ReplicateState::Active => self.as_mut().active_segment().poll_flush(cx),
            ReplicateState::Rollover(_, last_offset) => {
                let poll_resut = {
                    trace!("segment creation complete. switching as active");
                    match self.as_mut().active_segment().poll_roll_over(cx) {
                        Poll::Ready(Ok(_)) => {
                            let roll_over_ft = async {
                                trace!("segment creation complete. switching as active");
                                let option = &self.as_ref().option;
                                let new_segment =
                                    MutableSegment::create(last_offset, option).await?;
                                let old_mut_segment =
                                    mem::replace(&mut self.as_mut().active_segment, new_segment);
                                let old_segment = old_mut_segment.as_segment().await?;
                                self.as_mut().prev_segments().add_segment(old_segment);
                                let roll_over =
                                    mem::replace(&mut self.state, ReplicateState::Active);
                                Ok(roll_over.roll_over_item()) as Result<DefaultBatch, StorageError>
                            };
                            pin_mut!(roll_over_ft);
                            roll_over_ft.poll(cx)
                        }
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(err)) => return Poll::Ready(Err(err.into())),
                    }
                };

                match poll_resut {
                    Poll::Ready(Ok(batch)) => {
                        let pin_active_segment = self.as_mut().active_segment();
                        match pin_active_segment.start_send(batch) {
                            Ok(_) => {
                                trace!("now flushing");
                                let pin_active_segment = self.as_mut().active_segment();
                                pin_active_segment.poll_flush(cx)
                            }
                            Err(err) => return Poll::Ready(Err(err)),
                        }
                    }
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                }
            }
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        debug!("closing");
        match self.state {
            ReplicateState::Active => self.active_segment().poll_close(cx),
            _ => panic!("do not close on roll over"),
        }
    }
}

// generate replication folder name
fn replica_dir_name<S: AsRef<str>>(topic_name: S, partition_index: Size) -> String {
    format!("{}-{}", topic_name.as_ref(), partition_index)
}

#[cfg(test)]
mod tests {

    use futures::sink::SinkExt;
    use log::debug;
    use std::env::temp_dir;
    use std::fs;
    use std::fs::metadata;
    use std::io::Cursor;

    use future_helper::test_async;
    use kf_protocol::api::DefaultBatch;
    use kf_protocol::api::Offset;
    use kf_protocol::Decoder;
    use kf_protocol::Encoder;
    use kf_protocol::api::ErrorCode;
    use kf_socket::FilePartitionResponse;
    use kf_protocol::api::DefaultRecords;
    
    use super::FileReplica;
    use crate::fixture::create_batch;
    use crate::fixture::ensure_clean_dir;
    use crate::fixture::read_bytes_from_file;
    use crate::ConfigOption;
    use crate::StorageError;
    use crate::SegmentSlice;
    use crate::ReplicaStorage;
    

    const TEST_SEG_NAME: &str = "00000000000000000020.log";
    const TEST_SE2_NAME: &str = "00000000000000000022.log";
    const TEST_SEG_IDX: &str = "00000000000000000020.index";
    const TEST_SEG2_IDX: &str = "00000000000000000022.index";
    const START_OFFSET: Offset = 20;

    fn base_option(dir: &str) -> ConfigOption {

        let base_dir = temp_dir().join(dir);
        ensure_clean_dir(&base_dir);
        ConfigOption {
            segment_max_bytes: 10000,
            base_dir,
            index_max_interval_bytes: 1000,
            index_max_bytes: 1000,
            ..Default::default()
        }
    }

    fn rollover_option(dir: &str) -> ConfigOption {

        let base_dir = temp_dir().join(dir);
        ensure_clean_dir(&base_dir);
        ConfigOption {
            segment_max_bytes: 100,
            base_dir,
            index_max_bytes: 1000,
            index_max_interval_bytes: 0,
            ..Default::default()
        }
    }

    #[test_async]
    async fn test_replica_simple() -> Result<(), StorageError> {
        let option = base_option("testsimple");
        let mut replica = FileReplica::create("test", 0, START_OFFSET, &option).await.expect("test replica");

        assert_eq!(replica.get_log_start_offset(),START_OFFSET);
        replica.send(create_batch()).await?;
        replica.update_high_watermark(10).await?;

        let test_file = option.base_dir.join("test-0").join(TEST_SEG_NAME);
        debug!("using test file: {:#?}",test_file);
        let bytes = read_bytes_from_file(&test_file)?;

        let batch = DefaultBatch::decode_from(&mut Cursor::new(bytes),0)?;
        assert_eq!(batch.get_header().magic, 2, "check magic");
        assert_eq!(batch.get_base_offset(), START_OFFSET);
        assert_eq!(batch.get_header().last_offset_delta, 1);
        assert_eq!(batch.records.len(), 2);

         // find segment
        let segment = replica.find_segment(0);
        assert!(segment.is_none());

        let segment = replica.find_segment(20);
        match segment.unwrap() {
            SegmentSlice::MutableSegment(_) => assert!(true,"should be active segment"),
            SegmentSlice::Segment(_) => assert!(false,"cannot be inactive")
        }

        
        let segment = replica.find_segment(21);
        assert!(segment.is_some());
        match segment.unwrap() {
            SegmentSlice::MutableSegment(_) => assert!(true,"should be active segment"),
            SegmentSlice::Segment(_) => assert!(false,"cannot be inactive")
        }

        replica.send(create_batch()).await?;

        let segment = replica.find_segment(30);
        assert!(segment.is_some());
        match segment.unwrap() {
            SegmentSlice::MutableSegment(_) => assert!(true,"should be active segment"),
            SegmentSlice::Segment(_) => assert!(false,"cannot be inactive")
        }
        
        
        Ok(())
    }

    const TEST_UNCOMMIT_DIR: &str = "testuncommitted";

    #[test_async]
    async fn test_uncommited_fetch() -> Result<(), StorageError> {
        
        let option = base_option(TEST_UNCOMMIT_DIR);
       
        let mut replica = FileReplica::create("test", 0, 0, &option).await.expect("test replica");

        let mut empty_response = FilePartitionResponse::default();
        replica.read_uncommitted_records(&mut empty_response).await;
        assert_eq!(empty_response.records.len(),0);
        assert_eq!(empty_response.error_code,ErrorCode::None);

        
        let batch = create_batch();
        let batch_len = batch.write_size(0);
        debug!("batch len: {}",batch_len);
        replica.send(batch).await?;

        let mut partition_response = FilePartitionResponse::default();
        replica.read_uncommitted_records(&mut partition_response).await;
        assert_eq!(partition_response.records.len(),batch_len);

        replica.update_high_watermark(2).await?;  // first batch
        assert_eq!(replica.get_hw(),2);

        let batch = create_batch();
        let batch_len = batch.write_size(0);
        replica.send(batch).await?;

        let mut partition_response = FilePartitionResponse::default();
        replica.read_uncommitted_records(&mut partition_response).await;
        debug!("partiton response: {:#?}",partition_response);
        assert_eq!(partition_response.records.len(),batch_len);

        replica.send(create_batch()).await?;
         let mut partition_response = FilePartitionResponse::default();
        replica.read_uncommitted_records(&mut partition_response).await;
        assert_eq!(partition_response.records.len(),batch_len*2);
     
        Ok(())
    }

    const TEST_OFFSET_DIR: &str = "testoffset";

    #[test_async]
    async fn test_replica_end_offset() -> Result<(), StorageError> {

        let option = base_option(TEST_OFFSET_DIR);
       
        let mut rep_sink = FileReplica::create("test", 0, START_OFFSET, &option).await.expect("test replica");
        rep_sink.send(create_batch()).await?;
        rep_sink.send(create_batch()).await?;
        drop(rep_sink);

        // open replica
        let replica2 = FileReplica::create("test", 0, START_OFFSET, &option).await.expect("test replica");
        assert_eq!(replica2.get_leo(), START_OFFSET + 4);

        Ok(())
    }

    const TEST_REPLICA_DIR: &str = "test_replica";

    // you can show log by:  RUST_LOG=commit_log=debug cargo test roll_over

    #[test_async]
    async fn test_rep_log_roll_over() -> Result<(), StorageError> {

        let option = rollover_option(TEST_REPLICA_DIR);
       
        let mut replica =
            FileReplica::create("test", 1, START_OFFSET, &option).await.expect("create rep");

        // first batch
        debug!(">>>> sending first batch");
        let batches = create_batch();
        replica.send(batches).await?;

        // second batch
        debug!(">>>> sending second batch. this should rollover");
        let batches = create_batch();
        replica.send(batches).await?;
        debug!("finish sending next batch");

        assert_eq!(replica.get_log_start_offset(),START_OFFSET);
        let replica_dir = &option.base_dir.join("test-1");
        let dir_contents = fs::read_dir(&replica_dir)?;
        assert_eq!(dir_contents.count(), 5, "should be 5 files");

        
        let seg2_file = replica_dir.join(TEST_SE2_NAME);
        let bytes = read_bytes_from_file(&seg2_file)?;

        let batch = DefaultBatch::decode_from(&mut Cursor::new(bytes),0)?;
        assert_eq!(batch.get_header().magic, 2, "check magic");
        assert_eq!(batch.records.len(), 2);
        assert_eq!(batch.get_base_offset(), 22);

        
        let metadata_res = metadata(replica_dir.join(TEST_SEG2_IDX));
        assert!(metadata_res.is_ok());
        let metadata2 = metadata_res.unwrap();
        assert_eq!(metadata2.len(), 1000);

        
        let seg1_metadata = metadata(replica_dir.join(TEST_SEG_IDX))?;
        assert_eq!(seg1_metadata.len(), 8);
        

        Ok(())
    }


    const TEST_COMMIT_DIR: &str = "testcommit";
  
    #[test_async]
    async fn test_replica_commit() -> Result<(), StorageError> {
         
        let option = base_option(TEST_COMMIT_DIR);
        let mut replica = FileReplica::create("test", 0, 0, &option).await.expect("test replica");

        let records = DefaultRecords::default()
            .add(create_batch());

        replica.send_records(records,true).await?;
        
        // record contains 2 batch
        assert_eq!(replica.get_hw(),2);

        drop(replica);
        
        // restore replica
        let replica = FileReplica::create("test", 0, 0, &option).await.expect("test replica");
        assert_eq!(replica.get_hw(),2);
        
        Ok(())
    }


    
    const TEST_COMMIT_FETCH_DIR: &str = "testcommitt_fetch";

    /// test fetch only committed records
    #[test_async]
    async fn test_committed_fetch() -> Result<(), StorageError> {
        
        let option = base_option(TEST_COMMIT_FETCH_DIR);
       
        let mut replica = FileReplica::create("test", 0, 0, &option).await.expect("test replica");

        let batch = create_batch();
        let batch_len = batch.write_size(0);
        replica.send(batch).await.expect("writing records");

        let mut partition_response = FilePartitionResponse::default();
        replica.read_committed_records(0,&mut partition_response).await;
        debug!("partiton response: {:#?}",partition_response);
        assert_eq!(partition_response.records.len(),0); 

        replica.update_high_watermark_to_end().await.expect("update highwatermark");

        debug!("replica end: {} high: {}",replica.get_leo(),replica.get_hw());

        let mut partition_response = FilePartitionResponse::default();
        replica.read_committed_records(0,&mut partition_response).await;
        debug!("partiton response: {:#?}",partition_response);
        assert_eq!(partition_response.records.len(),batch_len); 

        // write 1 more batch
        let batch = create_batch();
        replica.send(batch).await.expect("writing 2nd batch");
        debug!("2nd batch: replica end: {} high: {}",replica.get_leo(),replica.get_hw());

        let mut partition_response = FilePartitionResponse::default();
        replica.read_committed_records(0,&mut partition_response).await;
        debug!("partiton response: {:#?}",partition_response);
        // should return same records as 1 batch since we didn't commit 2nd batch
        assert_eq!(partition_response.records.len(),batch_len); 

        Ok(())
    }
    

}
