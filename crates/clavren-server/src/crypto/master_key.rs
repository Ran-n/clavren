// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:29.616257
//+ Revised: 	2026/11/09 23:01:00.780229
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
mod tests {
    use super::*;

    fn found(file: MasterKeyFile) -> MasterKey {
        match file {
            MasterKeyFile::Found(key) => key,
            MasterKeyFile::Missing => panic!("expected a master key"),
        }
    }

    #[test]
    fn a_new_data_dir_has_no_key() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(read(dir.path()).unwrap(), MasterKeyFile::Missing));
    }

    #[test]
    fn a_created_key_reads_back_identically() {
        let dir = tempfile::tempdir().unwrap();
        let created = create(dir.path()).unwrap();
        assert_eq!(found(read(dir.path()).unwrap()).as_bytes(), created.as_bytes());
        assert_eq!(fs::read(master_key_path(dir.path())).unwrap().len(), MASTER_KEY_LENGTH);
    }

    #[test]
    fn the_data_dir_is_created_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("var").join("lib").join("clavren");
        create(&nested).unwrap();
        assert!(master_key_path(&nested).is_file());
    }

    #[test]
    fn two_created_keys_differ() {
        let (first, second) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        assert_ne!(create(first.path()).unwrap().as_bytes(), create(second.path()).unwrap().as_bytes());
    }

    fn file_names(dir: &Path) -> Vec<String> {
        fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().file_name().into_string().unwrap()).collect()
    }

    #[test]
    fn an_existing_key_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let original = create(dir.path()).unwrap();
        assert_eq!(create(dir.path()).unwrap().as_bytes(), original.as_bytes(), "the key on disk is the key");
        assert_eq!(found(read(dir.path()).unwrap()).as_bytes(), original.as_bytes());
        assert_eq!(file_names(dir.path()), ["master.key"], "no temporary file is left behind");
    }

    /// Regression: every process wrote `master.key.tmp` and renamed it over `master.key`, so the
    /// server and `clavren-admin` starting together could each keep a different key.
    #[test]
    fn processes_creating_the_key_together_all_get_the_same_one() {
        let dir = tempfile::tempdir().unwrap();
        let keys: Vec<_> = std::thread::scope(|scope| {
            let creators: Vec<_> = (0..16).map(|_| scope.spawn(|| *create(dir.path()).unwrap().as_bytes())).collect();
            creators.into_iter().map(|creator| creator.join().unwrap()).collect()
        });
        let on_disk = *found(read(dir.path()).unwrap()).as_bytes();
        assert!(keys.iter().all(|key| *key == on_disk));
        assert_eq!(file_names(dir.path()), ["master.key"]);
    }

    #[test]
    fn a_leftover_temporary_file_from_a_crash_is_no_obstacle() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("master.key.tmp"), b"half").unwrap();
        create(dir.path()).unwrap();
        assert!(matches!(read(dir.path()).unwrap(), MasterKeyFile::Found(_)));
    }

    #[test]
    fn a_key_of_the_wrong_length_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(master_key_path(dir.path()), [7u8; 31]).unwrap();
        let error = read(dir.path()).unwrap_err();
        assert!(matches!(error, MasterKeyError::WrongLength { length: 31, .. }));
        assert!(error.to_string().contains("restore"), "{error}");
    }

    #[test]
    fn a_directory_in_place_of_the_key_is_an_io_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(master_key_path(dir.path())).unwrap();
        assert!(matches!(read(dir.path()), Err(MasterKeyError::Io { .. })));
    }

    #[cfg(unix)]
    #[test]
    fn a_new_key_is_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("clavren");
        create(&data_dir).unwrap();
        let file_mode = fs::metadata(master_key_path(&data_dir)).unwrap().permissions().mode() & 0o777;
        let dir_mode = fs::metadata(&data_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!((file_mode, dir_mode), (0o600, 0o700));
    }

    #[cfg(unix)]
    #[test]
    fn a_key_readable_by_others_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        create(dir.path()).unwrap();
        fs::set_permissions(master_key_path(dir.path()), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(read(dir.path()), Err(MasterKeyError::ReadableByOthers { mode: 0o644, .. })));
    }

    #[test]
    fn debug_output_never_shows_the_key() {
        assert_eq!(format!("{:?}", MasterKey::from_bytes([0xAB; 32])), "MasterKey(<redacted>)");
    }
}
