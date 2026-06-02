use std::fs::{File, OpenOptions};
use std::path::PathBuf;

use fs2::FileExt;

pub(crate) struct HostTmpShatterLock {
    _file: File,
}

pub(crate) fn host_tmp_shatter_lock() -> HostTmpShatterLock {
    let path = PathBuf::from("/tmp/shatter_tmpdir_regression.lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    file.lock_exclusive()
        .unwrap_or_else(|e| panic!("lock {}: {e}", path.display()));
    HostTmpShatterLock { _file: file }
}
