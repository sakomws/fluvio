#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;



/// Slice of the file
/// This works only on raw fd
#[derive(Default,Debug)]
pub struct AsyncFileSlice {
    fd: RawFd,
    position: u64,
    len: u64
}

impl AsyncFileSlice {

    pub fn new(fd: RawFd,position: u64,len: u64) -> Self {
        Self {
            fd,
            position,
            len
        }
    }

    pub fn position(&self) -> u64 {
        self.position
    }

    pub fn len(&self) -> u64 {
        self.len
    }


}


impl AsRawFd for AsyncFileSlice {

    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }   
}

