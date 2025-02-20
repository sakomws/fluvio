
use std::io::Error as IoError;
use std::io::ErrorKind;

use log::debug;
use future_aio::fs::AsyncFile;
use future_aio::fs::AsyncFileSlice;

use kf_protocol::api::Offset;
use kf_protocol::api::Size;


use crate::util::generate_file_name;
use crate::validator::validate;
use crate::validator::LogValidationError;
use crate::ConfigOption;
use crate::StorageError;

pub(crate) const MESSAGE_LOG_EXTENSION: &'static str = "log";

/// Records stored in the file
pub(crate) trait FileRecords {

    fn get_base_offset(&self) -> Offset;

    fn get_file(&self) -> &AsyncFile;

    /// as file slice from position
    fn as_file_slice(&self, start: Size) -> Result<AsyncFileSlice,IoError>;

    fn as_file_slice_from_to(&self, start: Size, len: Size) -> Result<AsyncFileSlice,IoError>;

}

pub struct FileRecordsSlice {
    base_offset: Offset,
    file: AsyncFile,
    len: u64
}

impl FileRecordsSlice {
 
    pub async fn open(
        base_offset: Offset,
        option: &ConfigOption,
    ) -> Result<FileRecordsSlice, StorageError> {
        let log_path = generate_file_name(&option.base_dir, base_offset, MESSAGE_LOG_EXTENSION);
        debug!("opening commit log at: {}", log_path.display());

        let file = AsyncFile::open(log_path).await?;

        let metadata = file.metadata().await?;
        let len = metadata.len();

        Ok(FileRecordsSlice {
            base_offset,
            file,
            len
        })
    }

    pub fn get_base_offset(&self) -> Offset {
        self.base_offset
    }

    #[allow(dead_code)]
    pub async fn validate(&mut self) -> Result<Offset, LogValidationError> {
        validate(&mut self.file).await
    }
}

impl FileRecords for FileRecordsSlice {

    fn get_base_offset(&self) -> Offset {
        self.base_offset
    }

    fn get_file(&self) -> &AsyncFile {
        &self.file
    }

    
    fn as_file_slice(&self, start_pos: Size) -> Result<AsyncFileSlice,IoError> {
        Ok(self.file.raw_slice(start_pos as u64 ,self.len - start_pos as u64))
    }

    fn as_file_slice_from_to(&self, start: Size, len: Size) -> Result<AsyncFileSlice,IoError> {
        if len as u64 > self.len {
            Err(IoError::new(ErrorKind::UnexpectedEof,"len is smaller than actual len"))
        } else {
            Ok(self.file.raw_slice(start as u64 ,len as u64))
        }
      
    }
    

}

// message log doesn't have circular structure
impl Unpin for FileRecordsSlice {}

