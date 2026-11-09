[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/03 04:54:17.447469 )
[//]: # (+ Revised: 	2026/11/08 14:55:48.549705 )
[//]: # ( ---------------------------------------------------------------------- )

# Client integration

How a desktop application uses Clavren through the `clavren-client` crate: activating a key,
running offline, checking in once a day, and what to show for each outcome. The wire protocol
is in [api.md](api.md#application-api), for a client in another language.

---

## What the crate does

- **Activates** a key typed in any form (case, spaces and dashes don't matter) and binds it to
  this computer with a device key pair it generates.
- **Keeps its state** in `%APPDATA%\<vendor>\<product>\licence.dat`, protected with Windows DPAPI
  for the current user. A copied file is useless on another computer or account.
- **Works offline** for 30 days after its last successful check: the cached token is signed by
  Clavren and verified locally against the public keys the application embeds.
- **Checks in** once a day (when the last check is more than 24 hours old), at start or while
  running, on a background thread that never blocks the UI and retries with backoff (1, 5, 15,
  60 minutes) when the server can't be reached.
- **Reports outcomes**, never text: the application shows its own translated message for each.
- **Notices a clock set back** by more than an hour and asks for an online check instead of
  trusting it. Tokens are judged by the server's time (the system clock plus the difference
  measured at the last answer), so a computer whose clock is simply wrong works normally, and
  setting the clock back before a check doesn't stretch the 30 days.
- **Keeps nothing it can't use**: an answer that isn't Clavren's (a proxy's page) or a token it
  can't verify changes nothing, and the check is retried with backoff.

Logs go through the `log` crate; keys, tokens and device keys are never logged.

---

## Setup

```toml
[dependencies]
# The Clavren repository next to the application's
clavren-client = { path = "../clavren/crates/clavren-client" }
```

```rust
use std::sync::Arc;
use clavren_client::{Background, Client, Config, Outcome, trusted_keys};

const LICENCE_SERVER: &str = "https://licences.example.com";
// From `clavren-admin show-public-key`; see "Trusted keys" below.
const TOKEN_KEYS: &[(&str, &str)] = &[("v1", "kLx…base64url…")];

fn licence_client() -> Arc<Client> {
    let keys = trusted_keys(TOKEN_KEYS).expect("the embedded public keys are valid");
    let config = Config::new("avalio-lite", keys, env!("CARGO_PKG_VERSION"));
    Arc::new(Client::for_windows(config, LICENCE_SERVER, "Breren"))
}
```

`Config::new` names the device after the computer (`COMPUTERNAME`), sets the platform
(`windows-x86_64`) and the daily check interval. Every field is public; for a test build,
shorten `check_interval`.

---

## At start

`status()` reads the saved state and the cached token without any network access, so it is fast
enough for the UI thread:

```rust
let client = licence_client();
match client.status() {
    Outcome::Licensed { offline_until, .. } => run_app(offline_until),
    Outcome::NeedsActivation => show_activation_screen(),
    other => show_locked_screen(other),
}
let checks = Background::spawn(client.clone(), move |outcome| {
    // Runs on the checker's thread: hand the outcome to the UI thread.
    ui.post(move || apply_outcome(outcome));
})?;
// … at shutdown:
checks.stop();
```

The background checker calls `check()` whenever one is due and reports only when the outcome's
kind changes (a renewed token is not a change). `stop()` wakes it and waits for it to finish, so
the application shuts down cleanly; a check under way finishes first (two requests, up to 15
seconds each), so call `stop()` (or drop it) off the UI thread when that matters. A panic in the
callback is logged and checking goes on.

`status()` never touches the disk, so calling it often costs nothing. The time it saw (for
noticing a clock set back between runs) is saved by the background checker every ten minutes and
when it stops; an application that runs no checker calls `save_time_seen()` off the UI thread
now and then.

## Activating

```rust
// On a worker thread: it goes to the server.
match client.activate(&typed_key)? {
    Outcome::Licensed { .. } => unlock(),
    outcome => show_activation_error(outcome),
}
```

`activate` returns `Err` when the device key couldn't be saved (then nothing was sent), or when
the server accepted the activation but it couldn't be saved; retrying is then safe, because the
device key was saved first and the server knows the same computer again. When the saved state
existed but couldn't be read at start (another program had the file open), nothing is saved over
it and `activate` fails until the application starts again.

---

## Outcomes and what to show

| Outcome | Meaning | The application |
|---------|---------|-----------------|
| `Licensed { licence_expires_at, offline_until }` | Valid | Runs. May warn when `offline_until` is under 5 days away ("connect to the internet") or the licence expires soon |
| `NeedsActivation` | No licence on this computer | Asks for a key |
| `InvalidKey` | Malformed, unknown, or another product's key | Asks again |
| `AlreadyActivatedElsewhere` | The key is in use on another computer | Explains how to ask for a reset |
| `Revoked` | Revoked, or this computer was reset | Locks; shows the support contact. The local activation is cleared, so a new key can be entered |
| `Expired { at }` | The licence expired | Locks; shows the support contact. Checked again daily, so an extended licence unlocks by itself |
| `ProductUnavailable` | The product was deactivated | Locks. Checked again daily |
| `NeedsCheck` | 30 days offline, or the clock was set back | Asks to connect; unlocks after the next successful check |
| `RateLimited { retry_after }` | Too many attempts | Asks to wait |
| `Unreachable` | Activating needs the server, which can't be reached | Asks to try again later |

A temporary failure during a check (network, server error) changes nothing while the cached token
is valid.

---

## Trusted keys

Tokens are signed with a key derived from the server's master key, identified by its version
(`kid`: `v1`, `v2`…). The application embeds the public keys it trusts:

```sh
clavren-admin show-public-key
# v1 (in use): kLx…
# v2 (next):   p8Q…
```

To change the signing key without locking anyone out:

1. Ship a version that trusts both `v1` and `v2`.
2. When every copy in use has it, set `CLAVREN_SIGNING_KEY_VERSION=2` on the server
   ([deployment.md](deployment.md#changing-the-token-signing-key)).
3. Later versions may drop `v1`.

---

## Testing an integration

`Client::new` takes the store, transport and clock as traits, so an application's tests can run
the client with state in memory and a clock they move, against a test Clavren server (or their
own `Transport` that answers in memory):

```rust
use std::sync::Arc;
use clavren_client::{Client, clock::ManualClock, store::MemoryStore, transport::HttpTransport};

let clock = Arc::new(ManualClock::new(start));
let client = Client::new(config, Box::new(MemoryStore::default()), Box::new(HttpTransport::new(url)), Box::new(clock.clone()));
clock.set(start + chrono::TimeDelta::days(31)); // a month offline
```

The crate's own tests (`crates/clavren-client/tests/`) run it both against a fake server and
against the real Clavren server over HTTP: activate, check, reset, a second computer, revoke.
