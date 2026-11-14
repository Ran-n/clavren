// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:22.749636
//+ Revised: 	2026/11/14 12:05:22.749636
// -----------------------------------------------------------------------------

//! The saved state: unreadable, shared between two running copies, and when it is written.

use super::*;

/// Regression: every `status()` call saved the state (DPAPI and a file write on the UI thread).
/// Now `status()` never touches the disk; the time seen is saved apart, once it moved on.
#[test]
fn status_never_saves_and_the_time_seen_is_saved_once_it_moved_on() {
    struct Counting(MemoryStore, AtomicUsize);
    impl SecretStore for Counting {
        fn load(&self) -> Result<Option<Vec<u8>>, StoreError> {
            self.0.load()
        }
        fn save(&self, plain: &[u8]) -> Result<(), StoreError> {
            self.1.fetch_add(1, Ordering::SeqCst);
            self.0.save(plain)
        }
    }
    let computer = Computer::new();
    let store = Arc::new(Counting(MemoryStore::default(), AtomicUsize::new(0)));
    let client = computer.client_on(Box::new(store.clone()), Config::new(PRODUCT, FakeServer::trusted_keys(), "2.0.0"));
    assert!(client.activate(KEY).unwrap().is_licensed());
    let after_activation = store.1.load(Ordering::SeqCst);
    for _ in 0..100 {
        computer.advance(TimeDelta::seconds(1));
        assert!(client.status().is_licensed());
    }
    computer.advance(TimeDelta::hours(5));
    client.status();
    assert_eq!(store.1.load(Ordering::SeqCst), after_activation, "status() saves nothing");

    client.save_time_seen().unwrap();
    assert_eq!(store.1.load(Ordering::SeqCst), after_activation + 1);
    computer.advance(TimeDelta::minutes(9));
    client.status();
    client.save_time_seen().unwrap();
    assert_eq!(store.1.load(Ordering::SeqCst), after_activation + 1, "under ten minutes more: nothing to save");

    // the saved time catches a clock set back between runs
    computer.advance(TimeDelta::hours(-3));
    let restarted =
        computer.client_on(Box::new(store.clone()), Config::new(PRODUCT, FakeServer::trusted_keys(), "2.0.0"));
    assert_eq!(restarted.status(), Outcome::NeedsCheck);
}

#[test]
fn the_device_key_is_kept_so_retrying_is_the_same_computer() {
    let computer = Computer::new();
    computer.server.set_mode(Mode::Offline);
    computer.start().activate(KEY).unwrap();
    let saved = computer.store.load().unwrap().unwrap();
    computer.server.set_mode(Mode::Normal);
    assert!(computer.start().activate(KEY).unwrap().is_licensed());
    let key_of = |bytes: &[u8]| serde_json::from_slice::<serde_json::Value>(bytes).unwrap()["device_key"].clone();
    assert_eq!(key_of(&saved), key_of(&computer.store.load().unwrap().unwrap()));
}

#[test]
fn unreadable_state_starts_over() {
    struct Broken;
    impl SecretStore for Broken {
        fn load(&self) -> Result<Option<Vec<u8>>, StoreError> {
            Err(StoreError::Corrupt)
        }
        fn save(&self, _: &[u8]) -> Result<(), StoreError> {
            Err(StoreError::Corrupt)
        }
    }
    let computer = Computer::new();
    let config = Config::new(PRODUCT, FakeServer::trusted_keys(), "2.0.0");
    let client =
        Client::new(config, Box::new(Broken), Box::new(computer.server.clone()), Box::new(computer.clock.clone()));
    assert_eq!(client.status(), Outcome::NeedsActivation);
    assert!(client.activate(KEY).is_err(), "a device key that can't be saved stops the activation");
    assert_eq!(computer.server.calls.load(Ordering::SeqCst), 0);
    // Regression: the unsaved key stayed in memory, so trying again sent it, and the server
    // bound the licence to a device key this computer would have forgotten at its next start.
    assert!(client.activate(KEY).is_err(), "trying again doesn't get further");
    assert_eq!(computer.server.calls.load(Ordering::SeqCst), 0, "an unsaved device key is never sent");

    computer.store.save(b"not json").unwrap();
    assert_eq!(computer.start().status(), Outcome::NeedsActivation);
}

/// Regression: a saved licence that couldn't be read at start (another program had the file open)
/// counted as none, and the next activation saved over it.
#[test]
fn a_saved_licence_that_cannot_be_read_is_never_overwritten() {
    struct Locked(AtomicBool);
    impl SecretStore for Locked {
        fn load(&self) -> Result<Option<Vec<u8>>, StoreError> {
            let source = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "in use");
            Err(StoreError::Io { path: "licence.dat".into(), source })
        }
        fn save(&self, _: &[u8]) -> Result<(), StoreError> {
            self.0.store(true, Ordering::SeqCst);
            Ok(())
        }
    }
    let computer = Computer::new();
    let store = Arc::new(Locked(AtomicBool::new(false)));
    let client = computer.client_on(Box::new(store.clone()), Config::new(PRODUCT, FakeServer::trusted_keys(), "2.0.0"));
    assert_eq!(client.status(), Outcome::NeedsActivation);
    assert!(matches!(client.activate(KEY), Err(StoreError::Unreadable(_))));
    assert!(!store.0.load(Ordering::SeqCst), "nothing was saved over it");
    assert_eq!(computer.server.calls.load(Ordering::SeqCst), 0, "and nothing was sent");
}

/// Regression: each copy of the application saved its whole state as it had it in memory, so a
/// copy still at the key prompt saved over the licence another copy had just activated.
#[test]
fn a_running_copy_never_saves_over_a_licence_another_copy_activated() {
    let computer = Computer::new();
    let at_the_prompt = computer.start();
    assert_eq!(at_the_prompt.status(), Outcome::NeedsActivation);
    let other = computer.start();
    assert!(other.activate(KEY).unwrap().is_licensed());

    computer.advance(TimeDelta::minutes(30));
    at_the_prompt.status();
    assert!(matches!(at_the_prompt.save_time_seen(), Err(StoreError::ChangedElsewhere)));
    assert!(computer.start().status().is_licensed(), "the licence is still saved");
    assert!(at_the_prompt.status().is_licensed(), "and this copy took it over");
    computer.advance(TimeDelta::minutes(30));
    at_the_prompt.status();
    at_the_prompt.save_time_seen().unwrap();
    assert!(computer.start().status().is_licensed());
}

/// When what the other copy saved can't be read either, this copy keeps what it had and still
/// saves nothing over it.
#[test]
fn a_change_elsewhere_that_cannot_be_read_is_still_never_saved_over() {
    struct Clashing(AtomicUsize);
    impl SecretStore for Clashing {
        fn load(&self) -> Result<Option<Vec<u8>>, StoreError> {
            // nothing at the start; unreadable once the other copy has written
            if self.0.fetch_add(1, Ordering::SeqCst) == 0 { Ok(None) } else { Err(StoreError::Corrupt) }
        }
        fn save(&self, _: &[u8]) -> Result<(), StoreError> {
            panic!("saved over what another copy saved");
        }
        fn save_if_unchanged(&self, _: Option<&[u8]>, _: &[u8]) -> Result<bool, StoreError> {
            Ok(false)
        }
    }
    let computer = Computer::new();
    let config = Config::new(PRODUCT, FakeServer::trusted_keys(), "2.0.0");
    let client = computer.client_on(Box::new(Clashing(AtomicUsize::new(0))), config);
    assert!(matches!(client.activate(KEY), Err(StoreError::ChangedElsewhere)));
    assert_eq!(client.status(), Outcome::NeedsActivation);
    assert_eq!(computer.server.calls.load(Ordering::SeqCst), 0, "nothing was sent with an unsaved device key");
    assert!(matches!(client.activate(KEY), Err(StoreError::ChangedElsewhere)), "and it keeps refusing");
}

/// Two copies at the key prompt: the second to make a device key takes the first one's instead of
/// saving its own over it, so both activate as the same computer.
#[test]
fn two_copies_activating_end_up_as_one_computer() {
    let computer = Computer::new();
    let (first, second) = (computer.start(), computer.start());
    assert!(first.activate(KEY).unwrap().is_licensed());
    assert!(matches!(second.activate(KEY), Err(StoreError::ChangedElsewhere)));
    assert!(second.status().is_licensed(), "it runs on what the first copy saved");
    let calls = computer.server.calls.load(Ordering::SeqCst);
    assert!(second.activate(KEY).unwrap().is_licensed(), "trying again is the same computer again");
    assert!(computer.server.calls.load(Ordering::SeqCst) > calls);
    assert!(first.status().is_licensed());
}
