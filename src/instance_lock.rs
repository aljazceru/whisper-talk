use crate::error::{Result, WhisperTalkError};
use std::fs::File;
use std::os::unix::io::AsRawFd;

pub struct InstanceLock {
    file: Option<File>,
}

impl InstanceLock {
    pub fn acquire(lock_file_path: &std::path::Path) -> Result<Self> {
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(lock_file_path)
            .map_err(|e| WhisperTalkError::System(format!("Failed to open lock file: {}", e)))?;

        let fd = file.as_raw_fd();
        let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };

        if result != 0 {
            return Err(WhisperTalkError::AlreadyRunning);
        }

        Ok(Self { file: Some(file) })
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        if let Some(file) = &self.file {
            let fd = file.as_raw_fd();
            unsafe { libc::flock(fd, libc::LOCK_UN) };
        }
    }
}
