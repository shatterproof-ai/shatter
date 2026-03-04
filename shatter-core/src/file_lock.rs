//! Advisory file locking for `.shatter/` shared state files.
//!
//! Wraps `fs2::FileExt` (flock syscall) to prevent lost-update races when
//! multiple shatter processes run concurrently. Locks are kernel-managed —
//! released automatically on process exit, crash, or SIGKILL.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use fs2::FileExt;

/// RAII guard that holds an exclusive flock on a `.lock` sidecar file.
/// The lock is released when the guard is dropped.
pub struct FileLock {
    _file: File,
    lock_path: PathBuf,
}

impl FileLock {
    /// Acquire an exclusive lock for `protected_path` (blocking).
    ///
    /// Creates a sidecar `.lock` file next to the protected path.
    /// Blocks until the lock is available.
    pub fn acquire(protected_path: &Path) -> io::Result<Self> {
        let lock_path = lock_path_for(protected_path);
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(&lock_path)?;
        file.lock_exclusive()?;
        Ok(Self {
            _file: file,
            lock_path,
        })
    }

    /// Try to acquire an exclusive lock without blocking.
    ///
    /// Returns `Ok(None)` if another process holds the lock.
    pub fn try_acquire(protected_path: &Path) -> io::Result<Option<Self>> {
        let lock_path = lock_path_for(protected_path);
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(&lock_path)?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(Self {
                _file: file,
                lock_path,
            })),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            // fs2 on Linux returns Other for EWOULDBLOCK on some kernels
            Err(e) if e.raw_os_error() == Some(libc::EWOULDBLOCK) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Path of the lock file (for diagnostics).
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
}

/// Compute the `.lock` sidecar path for a given file.
///
/// `foo/bar.json` → `foo/bar.json.lock`
fn lock_path_for(path: &Path) -> PathBuf {
    let mut lock = path.as_os_str().to_owned();
    lock.push(".lock");
    PathBuf::from(lock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn acquire_and_drop_releases_lock() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("state.json");

        let guard = FileLock::acquire(&target).unwrap();
        assert!(guard.lock_path().exists());
        drop(guard);

        // Can re-acquire after drop
        let _guard2 = FileLock::acquire(&target).unwrap();
    }

    #[test]
    fn try_acquire_returns_none_when_held() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("state.json");

        let _guard = FileLock::acquire(&target).unwrap();
        let result = FileLock::try_acquire(&target).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn concurrent_access_serialized() {
        let dir = tempfile::tempdir().unwrap();
        let counter_path = dir.path().join("counter.json");
        std::fs::write(&counter_path, "0").unwrap();

        let n_threads = 8;
        let iterations = 50;
        let barrier = Arc::new(Barrier::new(n_threads));
        let counter_path = Arc::new(counter_path);

        let handles: Vec<_> = (0..n_threads)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                let path = Arc::clone(&counter_path);
                std::thread::spawn(move || {
                    barrier.wait();
                    for _ in 0..iterations {
                        let _lock = FileLock::acquire(&path).unwrap();
                        let val: u64 = std::fs::read_to_string(&*path)
                            .unwrap()
                            .trim()
                            .parse()
                            .unwrap();
                        std::fs::write(&*path, (val + 1).to_string()).unwrap();
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let final_val: u64 = std::fs::read_to_string(&*counter_path)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(
            final_val,
            (n_threads * iterations) as u64,
            "concurrent increments should not lose updates"
        );
    }
}
