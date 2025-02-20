use std::io::Error as IoError;

use futures::stream::StreamExt;
use log::warn;
use log::trace;
use std::fmt;

use future_aio::fs::AsyncFile;
use future_aio::fs::FileSinkError;
use kf_protocol::api::Offset;

use crate::BatchHeaderStream;
use crate::util::log_path_get_offset;
use crate::util::OffsetError;

#[derive(Debug)]
pub enum LogValidationError {
    InvalidExtension,
    LogNameError(OffsetError),
    FileSinkError(FileSinkError),
    IoError(IoError),
    BaseOffError,
    OffsetNotOrderedError,
    NoBatches,
    ExistingBatch
}


impl fmt::Display for LogValidationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::InvalidExtension => write!(f, "invalid extension"),
            Self::LogNameError(err) => write!(f,"{}",err),
            Self::FileSinkError(err) => write!(f,"{}",err),
            Self::IoError(err) => write!(f,"{}",err),
            Self::BaseOffError => write!(f,"base off error"),
            Self::OffsetNotOrderedError => write!(f,"offset not order"),
            Self::NoBatches => write!(f,"no batches"),
            Self::ExistingBatch => write!(f,"batch exist")
        }
    }
}




impl From<OffsetError> for LogValidationError {
    fn from(error: OffsetError) -> Self {
        LogValidationError::LogNameError(error)
    }
}

impl From<IoError> for LogValidationError {
    fn from(error: IoError) -> Self {
        LogValidationError::IoError(error)
    }
}

impl From<FileSinkError> for LogValidationError {
    fn from(error: FileSinkError) -> Self {
        LogValidationError::FileSinkError(error)
    }
}

/// validate the file and find last offset
/// if file is not valid then return error
#[allow(dead_code)]
pub async fn validate(file: &mut AsyncFile) -> Result<Offset, LogValidationError> {
    let base_offset = log_path_get_offset(file.get_path())?;
    let file_name = file.get_path().display().to_string();

    trace!(
        "validating file: {}, baseoffset: {}",
        file_name, base_offset
    );

    let file_clone = file.try_clone().await?;
    let mut batch_stream = BatchHeaderStream::new(file_clone);
    let mut end_offset: Offset = -1;

    while let Some(batch_pos) = batch_stream.next().await {
        let batch_base_offset = batch_pos.get_batch().get_base_offset();
        let header = batch_pos.get_batch().get_header();
        let offset_delta = header.last_offset_delta;

        trace!(
            "found batch base: {} offset delta: {}",
            batch_base_offset, offset_delta
        );

        if batch_base_offset < base_offset {
            warn!(
                "batch base offset: {} is less than base offset: {} path: {:#?}",
                batch_base_offset, base_offset, file_name
            );
            return Err(LogValidationError::BaseOffError);
        }

        if batch_base_offset <= end_offset {
            warn!(
                "batch offset is  {} is less than prev offset  {}",
                batch_base_offset, end_offset
            );
            return Err(LogValidationError::OffsetNotOrderedError);
        }

        end_offset = batch_base_offset + offset_delta as Offset;
    }

    if let Some(err) = batch_stream.invalid() {
        return Err(err.into());
    }

    if end_offset == -1 {
        trace!("no batch found, returning last offset delta 0");
        return Ok(base_offset)
    }

    trace!("end offset: {}",end_offset);
    Ok(end_offset + 1)
}

#[cfg(test)]
mod tests {

    use std::env::temp_dir;

    use futures::sink::SinkExt;

    use future_aio::fs::AsyncFile;
    use future_aio::fs::FileSink;
    use future_aio::fs::FileSinkOption;
    use future_helper::test_async;
    use kf_protocol::api::DefaultRecord;
    use kf_protocol::api::DefaultBatch;
    use kf_protocol::api::Offset;

    use crate::fixture::ensure_clean_file;
    use crate::mut_records::MutFileRecords;
    use crate::ConfigOption;

    use super::validate;
    use crate::StorageError;

   
    const PRODUCER: i64 = 33;

    pub fn create_batch(base_offset: Offset, records: u16) -> DefaultBatch {
        let mut batches = DefaultBatch::default();
        batches.set_base_offset(base_offset);
        let header = batches.get_mut_header();
        header.magic = 2;
        header.producer_id = PRODUCER;
        header.producer_epoch = -1;
     
        for _ in 0..records {
            let mut record = DefaultRecord::default();
            let bytes: Vec<u8> = vec![10, 20];
            record.value = Some(bytes).into();
            batches.add_record(record);
        }
    
        batches
    }

    const TEST_FILE_NAME: &str = "00000000000000000301.log"; // for offset 301
    const BASE_OFFSET: Offset = 301;

    #[test_async]
    async fn test_validate_empty() -> Result<(), StorageError> {
        let test_file = temp_dir().join(TEST_FILE_NAME);
        ensure_clean_file(&test_file);

        let options = ConfigOption {
            base_dir: temp_dir(),
            segment_max_bytes: 1000,
            ..Default::default()
        };

        let _ = MutFileRecords::open(BASE_OFFSET, &options).await?;
        let mut file = AsyncFile::open(&test_file).await?;
        let next_offset = validate(&mut file).await?;
        assert_eq!(next_offset,BASE_OFFSET);

        Ok(())
    }

    const TEST_FILE_SUCCESS_NAME: &str = "00000000000000000601.log"; // for offset 301
    const SUCCESS_BASE_OFFSET: Offset = 601;
    

    #[test_async]
    async fn test_validate_success() -> Result<(), StorageError> {
        let test_file = temp_dir().join(TEST_FILE_SUCCESS_NAME);
        ensure_clean_file(&test_file);

        let options = ConfigOption {
            base_dir: temp_dir(),
            segment_max_bytes: 1000,
            ..Default::default()
        };

        let mut msg_sink = MutFileRecords::create(SUCCESS_BASE_OFFSET, &options).await?;

        msg_sink.send(create_batch(SUCCESS_BASE_OFFSET, 2)).await?;
        msg_sink.send(create_batch(SUCCESS_BASE_OFFSET+2,3)).await?;

        let mut file = AsyncFile::open(&test_file).await?;

        let next_offset = validate(&mut file).await?;
        assert_eq!(next_offset, SUCCESS_BASE_OFFSET + 5);

        Ok(())
    }

    const TEST_FILE_NAME_FAIL: &str = "00000000000000000401.log"; // for offset 301

    #[test_async]
    async fn test_validate_offset() -> Result<(), StorageError> {
        let test_file = temp_dir().join(TEST_FILE_NAME_FAIL);
        ensure_clean_file(&test_file);

        let options = ConfigOption {
            base_dir: temp_dir(),
            segment_max_bytes: 1000,
            ..Default::default()
        };

        let mut msg_sink = MutFileRecords::create(401, &options).await?;

        msg_sink.send(create_batch(401, 0)).await?;
        msg_sink.send(create_batch(111, 1)).await?;

        let mut file = AsyncFile::open(&test_file).await?;

        assert!(validate(&mut file).await.is_err());

        Ok(())
    }

    const TEST_FILE_NAME_FAIL2: &str = "00000000000000000501.log"; // for offset 301

    #[test_async]
    async fn test_validate_invalid_contents() -> Result<(), StorageError> {
        let test_file = temp_dir().join(TEST_FILE_NAME_FAIL2);
        ensure_clean_file(&test_file);

        let options = ConfigOption {
            base_dir: temp_dir(),
            segment_max_bytes: 1000,
            ..Default::default()
        };

        let mut msg_sink = MutFileRecords::create(501, &options).await?;
        msg_sink.send(create_batch(501, 2)).await?;

        // add some junk
        let mut f_sink = FileSink::create(&test_file, FileSinkOption::default()).await?;
        let bytes = vec![0x01, 0x02, 0x03];
        f_sink.send(bytes).await?;

        let mut file = AsyncFile::open(&test_file).await?;

        assert!(validate(&mut file).await.is_err());

        Ok(())
    }

}
