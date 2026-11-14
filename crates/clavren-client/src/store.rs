// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:11.718170
//+ Revised: 	2026/11/14 11:36:29.622142
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
    /// Saves `plain` only while what is stored is still `expected` (what this program last loaded
    /// or saved); `false`, saving nothing, when another program changed it meanwhile. A store
    /// shared between programs overrides this to compare and save in one step.
    fn save_if_unchanged(&self, expected: Option<&[u8]>, plain: &[u8]) -> Result<bool, StoreError> {
        if self.load()?.as_deref() != expected {
            return Ok(false);
        }
        self.save(plain).map(|()| true)
    }
}

/// A shared store is a store: a test can restart the client on the same one.
impl<T: SecretStore + ?Sized> SecretStore for std::sync::Arc<T> {
    fn load(&self) -> Result<Option<Vec<u8>>, StoreError> {
        (**self).load()
    }

    fn save(&self, plain: &[u8]) -> Result<(), StoreError> {
        (**self).save(plain)
    }

    fn save_if_unchanged(&self, expected: Option<&[u8]>, plain: &[u8]) -> Result<bool, StoreError> {
        (**self).save_if_unchanged(expected, plain)
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
    /// Another copy of the application saved the licence since this one last read it. This copy
    /// has loaded what the other one saved; whatever it was doing can be tried again.
    #[error("another copy of the application changed the saved licence; it was loaded again, so try once more")]
    ChangedElsewhere,
}

/// A file, protected with `protector`. Writes go to a temporary file of their own first, flushed to
/// the disk, then replace the old one, so neither a crash nor a power cut leaves half a file.
/// Reads and writes hold a lock on `<name>.lock` next to it, so two programs sharing the file (two
/// copies of the application) never read or replace it halfway through each other's write, and
/// one never saves over what the other saved without having read it
/// ([`SecretStore::save_if_unchanged`]).
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
        self.read()
    }

    fn save(&self, plain: &[u8]) -> Result<(), StoreError> {
        let _lock = self.lock(true)?;
        self.write(plain)
    }

    fn save_if_unchanged(&self, expected: Option<&[u8]>, plain: &[u8]) -> Result<bool, StoreError> {
        // one lock over the comparison and the write: nothing changes the file in between
        let _lock = self.lock(true)?;
        if self.read()?.as_deref() != expected {
            return Ok(false);
        }
        self.write(plain).map(|()| true)
    }
}

impl FileStore {
    /// The stored state, unprotected; the caller holds the lock (or reads without it).
    fn read(&self) -> Result<Option<Vec<u8>>, StoreError> {
        match std::fs::read(&self.path) {
            Ok(protected) => self.protector.unprotect(&protected).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(self.io(error)),
        }
    }

    /// Replaces the file with `plain`, protected; the caller holds the exclusive lock.
    fn write(&self, plain: &[u8]) -> Result<(), StoreError> {
        let protected = self.protector.protect(plain)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| self.io(error))?;
        }
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
mod tests;
