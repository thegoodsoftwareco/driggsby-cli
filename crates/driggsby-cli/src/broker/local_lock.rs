use std::{fs::File, fs::OpenOptions};

use anyhow::Result;
use fs2::FileExt;

use crate::runtime_paths::RuntimePaths;

pub struct LocalStateLock {
    _file: File,
}

impl LocalStateLock {
    pub fn acquire(runtime_paths: &RuntimePaths) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&runtime_paths.lock_path)?;
        file.lock_exclusive()?;
        Ok(Self { _file: file })
    }
}
