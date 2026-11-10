// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:11.718170
//+ Revised: 	2026/11/09 23:01:00.706374
// -----------------------------------------------------------------------------

//! Where the client's state lives between runs: a file whose contents are protected by the
//! operating system. On Windows that is DPAPI in CurrentUser scope, so the file is useless to
//! another Windows account or another computer. Other systems plug in their own [`Protector`]
//! (macOS Keychain, Linux secret storage) later.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Makes bytes readable only by the current user on this computer, and back.
pub trait Protector: Send + Sync {
    fn protect(&self, plain: &[u8]) -> Result<Vec<u8>, StoreError>;
    fn unprotect(&self, protected: &[u8]) -> Result<Vec<u8>, StoreError>;
}

/// Somewhere to keep the client's state.
pub trait SecretStore: Send + Sync {
    /// `None` when nothing was saved yet.
    fn load(&self) -> Result<Option<Vec<u8>>, StoreError>;
    fn save(&self, plain: &[u8]) -> Result<(), StoreError>;
}

/// A shared store is a store: a test can restart the client on the same one.
impl<T: SecretStore + ?Sized> SecretStore for std::sync::Arc<T> {
    fn load(&self) -> Result<Option<Vec<u8>>, StoreError> {
        (**self).load()
    }

    fn save(&self, plain: &[u8]) -> Result<(), StoreError> {
        (**self).save(plain)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("the operating system refused to protect the licence data: {0}")]
    Protect(std::io::Error),
    #[error("the licence data couldn't be unprotected (another user, another computer, or damaged): {0}")]
    Unprotect(std::io::Error),
    #[error("the licence data is damaged")]
    Corrupt,
    /// The saved state existed but couldn't be read when the application started (e.g. another
    /// program had the file open); saving now would overwrite a licence that may still be good.
    #[error(
        "the saved licence couldn't be read at start ({0}); close other programs using it and restart the application, \
         or check that this Windows account may read the file"
    )]
    Unreadable(String),
}

/// A file, protected with `protector`. Writes go to a temporary file of their own first, flushed to
/// the disk, then replace the old one, so neither a crash nor a power cut leaves half a file.
/// Reads and writes hold a lock on `<name>.lock` next to it, so two programs sharing the file (two
/// copies of the application) never read or replace it halfway through each other's write.
pub struct FileStore {
    path: PathBuf,
    protector: Box<dyn Protector>,
}

impl FileStore {
    pub fn new(path: PathBuf, protector: Box<dyn Protector>) -> Self {
        Self { path, protector }
    }

    fn io(&self, source: std::io::Error) -> StoreError {
        StoreError::Io { path: self.path.clone(), source }
    }

    /// Holds the lock file until dropped: shared for reading, exclusive for writing.
    fn lock(&self, exclusive: bool) -> Result<File, StoreError> {
        let path = self.path.with_extension("lock");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| self.io(error))?;
        }
        let file =
            File::options().create(true).truncate(false).write(true).open(&path).map_err(|error| self.io(error))?;
        if exclusive { file.lock() } else { file.lock_shared() }.map_err(|error| self.io(error))?;
        Ok(file)
    }
}

/// A temporary name no other write uses, in this program or another one.
fn temporary_path(path: &Path) -> PathBuf {
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let write = WRITES.fetch_add(1, Ordering::Relaxed);
    path.with_extension(format!("{}-{write}.tmp", std::process::id()))
}

impl SecretStore for FileStore {
    fn load(&self) -> Result<Option<Vec<u8>>, StoreError> {
        // a lock that can't be taken (a read-only folder) mustn't make a good licence unreadable;
        // reading without it risks only a write happening at the same moment
        let _lock = self
            .lock(false)
            .inspect_err(|error| log::warn!("reading the licence state without its lock: {error}"))
            .ok();
        match std::fs::read(&self.path) {
            Ok(protected) => self.protector.unprotect(&protected).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(self.io(error)),
        }
    }

    fn save(&self, plain: &[u8]) -> Result<(), StoreError> {
        let protected = self.protector.protect(plain)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| self.io(error))?;
        }
        let _lock = self.lock(true)?;
        let temporary = temporary_path(&self.path);
        let written = File::create_new(&temporary)
            .and_then(|mut file| file.write_all(&protected).and_then(|()| file.sync_all()))
            .and_then(|()| std::fs::rename(&temporary, &self.path));
        if let Err(error) = written {
            if let Err(cleanup) = std::fs::remove_file(&temporary)
                && cleanup.kind() != std::io::ErrorKind::NotFound
            {
                log::warn!("couldn't remove {}: {cleanup}", temporary.display());
            }
            return Err(self.io(error));
        }
        Ok(())
    }
}

/// Keeps the state in memory only: for tests, and for applications' own tests.
#[derive(Default)]
pub struct MemoryStore(Mutex<Option<Vec<u8>>>);

impl SecretStore for MemoryStore {
    fn load(&self) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone())
    }

    fn save(&self, plain: &[u8]) -> Result<(), StoreError> {
        *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(plain.to_vec());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reverses the bytes: enough to tell protected from plain in a test.
    struct Reverse;

    impl Protector for Reverse {
        fn protect(&self, plain: &[u8]) -> Result<Vec<u8>, StoreError> {
            Ok(plain.iter().rev().copied().collect())
        }

        fn unprotect(&self, protected: &[u8]) -> Result<Vec<u8>, StoreError> {
            self.protect(protected)
        }
    }

    #[test]
    fn a_file_store_keeps_only_protected_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested").join("licence.dat");
        let store = FileStore::new(path.clone(), Box::new(Reverse));
        assert!(store.load().unwrap().is_none());
        store.save(b"state").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"etats");
        assert_eq!(store.load().unwrap().unwrap(), b"state");
        let mut left: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["licence.dat", "licence.lock"], "no temporary file is left behind");
    }

    /// Regression: every save used to write `licence.tmp`, so two copies of an application saving
    /// at once wrote into the same temporary file.
    #[test]
    fn saves_from_many_threads_never_mix() {
        let directory = tempfile::tempdir().unwrap();
        let store = std::sync::Arc::new(FileStore::new(directory.path().join("licence.dat"), Box::new(Reverse)));
        let writers: Vec<_> = (0..8u8)
            .map(|writer| {
                let store = store.clone();
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        store.save(&[writer; 4096]).unwrap();
                    }
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        let saved = store.load().unwrap().unwrap();
        assert_eq!(saved.len(), 4096);
        assert!(saved.iter().all(|byte| *byte == saved[0]), "one whole save, not a mix of two");
    }

    #[test]
    fn a_lock_that_cant_be_taken_still_reads() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("licence.dat");
        let store = FileStore::new(path.clone(), Box::new(Reverse));
        store.save(b"state").unwrap();
        // a folder where the lock file should be: it can't be opened
        std::fs::remove_file(path.with_extension("lock")).unwrap();
        std::fs::create_dir(path.with_extension("lock")).unwrap();
        assert_eq!(store.load().unwrap().unwrap(), b"state");
        assert!(matches!(store.save(b"other"), Err(StoreError::Io { .. })), "writing still needs it");
    }

    #[test]
    fn temporary_names_never_repeat() {
        let path = Path::new("licence.dat");
        assert_ne!(temporary_path(path), temporary_path(path));
    }

    #[test]
    fn a_failed_write_leaves_no_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        // a folder where the file should be: the final rename fails
        let path = directory.path().join("licence.dat");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("inside"), b"x").unwrap();
        let store = FileStore::new(path, Box::new(Reverse));
        assert!(matches!(store.save(b"state"), Err(StoreError::Io { .. })));
        let left: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn a_file_that_cant_be_read_is_an_error_naming_it_not_nothing() {
        let directory = tempfile::tempdir().unwrap();
        // a folder where the file should be: it exists, but can't be read as one
        let store = FileStore::new(directory.path().to_path_buf(), Box::new(Reverse));
        let error = store.load().unwrap_err();
        assert!(matches!(error, StoreError::Io { .. }), "{error}");
        assert!(error.to_string().contains(&directory.path().display().to_string()), "{error}");
    }

    #[test]
    fn a_memory_store_round_trips() {
        let store = MemoryStore::default();
        assert!(store.load().unwrap().is_none());
        store.save(b"x").unwrap();
        assert_eq!(store.load().unwrap().unwrap(), b"x");
    }
}
