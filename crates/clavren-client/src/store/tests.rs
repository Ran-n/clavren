// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:36:29.524410
//+ Revised: 	2026/11/14 11:36:29.524410
// -----------------------------------------------------------------------------

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

/// Regression: a save replaced whatever another copy of the application had saved since this
/// one read the file.
#[test]
fn a_save_never_replaces_what_another_program_saved_meanwhile() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("licence.dat");
    let (ours, theirs) = (FileStore::new(path.clone(), Box::new(Reverse)), FileStore::new(path, Box::new(Reverse)));
    assert!(ours.save_if_unchanged(None, b"first").unwrap(), "nothing stored, as expected");
    assert!(!theirs.save_if_unchanged(None, b"clash").unwrap(), "the file appeared meanwhile");
    assert!(theirs.save_if_unchanged(Some(b"first"), b"second").unwrap());
    assert!(!ours.save_if_unchanged(Some(b"first"), b"stale").unwrap());
    assert_eq!(ours.load().unwrap().unwrap(), b"second", "a refused save writes nothing");
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
    let shared = std::sync::Arc::new(store);
    assert!(!shared.save_if_unchanged(None, b"y").unwrap());
    assert!(shared.save_if_unchanged(Some(b"x"), b"y").unwrap());
    assert_eq!(shared.load().unwrap().unwrap(), b"y");
}
