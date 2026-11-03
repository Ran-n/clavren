// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:11.718170
//+ Revised: 	2026/11/03 04:54:11.718170
// -----------------------------------------------------------------------------

//! Where the client's state lives between runs: a file whose contents are protected by the
//! operating system. On Windows that is DPAPI in CurrentUser scope, so the file is useless to
//! another Windows account or another computer. Other systems plug in their own [`Protector`]
//! (macOS Keychain, Linux secret storage) later.

use std::path::PathBuf;
use std::sync::Mutex;

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
}

/// A file, protected with `protector`. Writes go to a temporary file first, then replace the old
/// one, so a crash never leaves half a file.
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
}

impl SecretStore for FileStore {
    fn load(&self) -> Result<Option<Vec<u8>>, StoreError> {
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
        let temporary = self.path.with_extension("tmp");
        std::fs::write(&temporary, protected).map_err(|error| self.io(error))?;
        std::fs::rename(&temporary, &self.path).map_err(|error| self.io(error))
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
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn a_memory_store_round_trips() {
        let store = MemoryStore::default();
        assert!(store.load().unwrap().is_none());
        store.save(b"x").unwrap();
        assert_eq!(store.load().unwrap().unwrap(), b"x");
    }
}
