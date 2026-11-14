// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:29.616257
//+ Revised: 	2026/11/14 11:36:29.804997
// -----------------------------------------------------------------------------

//! The master key: 32 random bytes in `<data dir>/master.key`, created on first start. Every other
//! key is derived from it, so losing it makes the stored licence keys and admin TOTP secrets
//! unreadable. That's why a missing file is only replaced when the database holds nothing
//! encrypted with it (the decision is in `startup`).

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

use super::random::{RandomError, random_bytes};

pub const MASTER_KEY_FILE: &str = "master.key";
pub const MASTER_KEY_LENGTH: usize = 32;
const TEMPORARY_SUFFIX: &str = ".tmp";

pub struct MasterKey(Zeroizing<[u8; MASTER_KEY_LENGTH]>);

impl MasterKey {
    pub fn from_bytes(bytes: [u8; MASTER_KEY_LENGTH]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; MASTER_KEY_LENGTH] {
        &self.0
    }
}

impl fmt::Debug for MasterKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MasterKey(<redacted>)")
    }
}

/// What the data directory holds.
#[derive(Debug)]
pub enum MasterKeyFile {
    Found(MasterKey),
    Missing,
}

#[derive(Debug, thiserror::Error)]
pub enum MasterKeyError {
    #[error("could not access {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error(
        "{} is {length} bytes long, but a master key is exactly {MASTER_KEY_LENGTH}; restore the master.key backed up \
         together with this database",
        path.display()
    )]
    WrongLength { path: PathBuf, length: usize },
    #[error("{} can be read by other users (mode {mode:o}); run `chmod 600 {}`", path.display(), path.display())]
    ReadableByOthers { path: PathBuf, mode: u32 },
    #[error(
        "{} is missing, but the database holds licence keys or admin accounts encrypted with it; restore master.key \
         from the backup taken together with this database (Clavren will not create a new one, it would make that \
         data unreadable)",
        path.display()
    )]
    MissingWithEncryptedData { path: PathBuf },
    #[error(transparent)]
    Random(#[from] RandomError),
}

pub fn master_key_path(data_dir: &Path) -> PathBuf {
    data_dir.join(MASTER_KEY_FILE)
}

/// Reads the master key, or reports that there is none yet.
pub fn read(data_dir: &Path) -> Result<MasterKeyFile, MasterKeyError> {
    let path = master_key_path(data_dir);
    let bytes = match fs::read(&path) {
        Ok(bytes) => Zeroizing::new(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(MasterKeyFile::Missing),
        Err(source) => return Err(MasterKeyError::Io { path, source }),
    };
    ensure_private(&path)?;
    let key: [u8; MASTER_KEY_LENGTH] =
        bytes.as_slice().try_into().map_err(|_| MasterKeyError::WrongLength { path, length: bytes.len() })?;
    Ok(MasterKeyFile::Found(MasterKey::from_bytes(key)))
}

/// Creates a new master key, or returns the one another process (the server and `clavren-admin`
/// starting together) created first: an existing key is never overwritten. The key is written
/// whole to a temporary file of this process's own, then linked to its final name, which fails
/// rather than replace a file already there; so a crash can't leave half a key behind either.
pub fn create(data_dir: &Path) -> Result<MasterKey, MasterKeyError> {
    create_private_dir(data_dir)?;
    let key = MasterKey::from_bytes(random_bytes::<MASTER_KEY_LENGTH>()?);
    let path = master_key_path(data_dir);
    let unique = encode_hex(&random_bytes::<8>()?);
    let temporary = data_dir.join(format!("{MASTER_KEY_FILE}.{unique}{TEMPORARY_SUFFIX}"));
    let io_error = |path: &Path| {
        let path = path.to_path_buf();
        move |source| MasterKeyError::Io { path, source }
    };
    write_private_file(&temporary, key.as_bytes()).map_err(io_error(&temporary))?;
    let mut linked = fs::hard_link(&temporary, &path);
    remove_if_present(&temporary).map_err(io_error(&temporary))?;
    if linked.as_ref().is_err_and(|error| error.kind() != io::ErrorKind::AlreadyExists) {
        // a file system without hard links (FAT, some network shares): create the key in place,
        // still never over another; a crash halfway leaves a short key, which is refused on read
        linked = write_private_file(&path, key.as_bytes());
    }
    match linked {
        Ok(()) => Ok(key),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => match read(data_dir)? {
            MasterKeyFile::Found(existing) => Ok(existing),
            MasterKeyFile::Missing => Err(MasterKeyError::Io { path, source: error }),
        },
        Err(source) => Err(MasterKeyError::Io { path, source }),
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

fn write_private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn create_private_dir(data_dir: &Path) -> Result<(), MasterKeyError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(data_dir).map_err(|source| MasterKeyError::Io { path: data_dir.to_path_buf(), source })
}

/// On Unix the key must be readable by its owner only. Windows (development only) has no mode
/// bits to check.
#[cfg(unix)]
fn ensure_private(path: &Path) -> Result<(), MasterKeyError> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path).map_err(|source| MasterKeyError::Io { path: path.to_path_buf(), source })?;
    let mode = metadata.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(MasterKeyError::ReadableByOthers { path: path.to_path_buf(), mode });
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private(_path: &Path) -> Result<(), MasterKeyError> {
    Ok(())
}

#[cfg(test)]
mod tests;
