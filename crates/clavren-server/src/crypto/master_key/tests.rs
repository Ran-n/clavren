// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:36:29.715372
//+ Revised: 	2026/11/15 00:08:08.148787
// -----------------------------------------------------------------------------

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
    // a plain write leaves it readable by others, which is refused before its length is looked at
    #[cfg(unix)]
    fs::set_permissions(
        master_key_path(dir.path()),
        <fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o600),
    )
    .unwrap();
    let error = read(dir.path()).unwrap_err();
    assert!(matches!(error, MasterKeyError::WrongLength { length: 31, .. }), "{error}");
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
