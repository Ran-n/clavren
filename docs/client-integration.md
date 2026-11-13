[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/03 04:54:17.447469 )
[//]: # (+ Revised: 	2026/11/12 23:57:46.367779 )
[//]: # ( ---------------------------------------------------------------------- )

# Client integration

How a desktop application uses Clavren through the `clavren-client` crate: activating a key,
running offline, checking in once a day, and what to show for each outcome. An application in
another language follows [Without the crate](#without-the-crate-another-language), with the wire
protocol in [api.md](api.md#application-api).

---

## What the crate does

- **Activates** a key typed in any form (case, spaces and dashes don't matter) and binds it to
  this computer with a device key pair it generates.
- **Keeps its state** in `%APPDATA%\<vendor>\<product>\licence.dat` (the temporary folder when
  `APPDATA` isn't set), protected with Windows DPAPI for the current user. A copied file is
  useless on another computer or account. Every write goes to a temporary file of its own and
  replaces the old one whole, under a lock (`licence.lock`), so two copies of the application
  never leave half a file.
- **Works offline** for its product's offline days after its last successful check (30 unless
  the administrator set 2 to 3,650 on the Products page): the cached token is signed by
  Clavren and verified locally against the public keys the application embeds.
- **Checks in** once a day (when the last check is more than 24 hours old), at start or while
  running, on a background thread that never blocks the UI and retries with backoff (1, 5, 15,
  60 minutes) when the server can't be reached.
- **Reports outcomes**, never text: the application shows its own translated message for each.
- **Notices a clock set back** by more than an hour, at once or a little at a time (the steps
  back since the last check add up, across restarts too), and asks for an online check instead of
  trusting it. Tokens are judged by the server's time (the system clock plus the difference
  measured from the signed token's issue time at the last check), so a computer whose clock is
  simply wrong works normally, and setting the clock back before a check doesn't stretch the 30
  days. A clock that was ahead and is put right by more than an hour needs one online check.
- **Talks HTTPS only**: `HttpTransport` refuses a plain `http://` server address unless it is
  this computer (`localhost`, `127.0.0.1`, `::1`), because the key travels in the activation.
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

### How to get a key

The key prompt shows a **How to get a key** link that opens `client.get_key_url()` in the default
browser. That is the product's key page on the licence server, `<server>/apply?product=<code>`,
where the administrator writes how to get a key (on the dashboard's Products page) and, when the
product takes applications, people apply. Keep no instructions of your own in the application:
the page's text changes for every installed copy at once, with no release. `get_key_url()` never
goes online, so it can be called on the UI thread.

```rust
if ui.link("How to get a key").clicked() {
    open::that(client.get_key_url())?; // any way of opening the default browser
}
```

---

## Applying for a licence

When a product takes applications (opened on the dashboard's Products page), the key prompt can
offer **Apply for a licence** next to **Enter a key**:

```rust
use clavren_client::{Applicant, Applied};

let applicant = Applicant { name, email, message: Some(message) };
match client.apply(&applicant)? {
    Applied::Sent => { /* "Sent. The licence is activated here once it is approved." */ }
    Applied::AlreadyPending => { /* "This address already has an application waiting." */ }
    Applied::InvalidDetails => { /* "Check the name and email." */ }
    Applied::ProductUnavailable => { /* "Applications are closed." */ }
    Applied::RateLimited { retry_after } => { /* "Wait and try again." */ }
    Applied::AlreadyLicensed | Applied::Unreachable => { /* … */ }
}
```

Run it off the UI thread: it goes online. While it waits, `status()` says `ApplicationPending`
(across restarts too) and the background checker asks every 15 minutes
(`Config::application_check_interval`); `check()` asks at once. Once approved, the client activates
the key itself, so the applicant never sees or types it, and the checker's callback gets
`Licensed`. A rejection is `ApplicationRejected { reason }`: show the administrator's reason, then
offer to apply again or to type a key (`dismiss_application()` goes back to `NeedsActivation`).
Typing a key while an application waits ends the application on this computer.

The applicant's name and email go to the licence server; say so where you ask for them. People
without the application can apply on the product's key page, `client.get_key_url()`.

Without the crate: `POST /api/v1/applications` with `"source": "app"`, keep the `secret` in the
protected state, then `POST /api/v1/applications/status` with it now and then, and activate the
`key` of an `approved` answer ([api.md](api.md#licence-applications)).

## Outcomes and what to show

| Outcome | Meaning | The application |
|---------|---------|-----------------|
| `Licensed { licence_expires_at, offline_until, test }` | Valid | Runs. May warn when `offline_until` is under 5 days away ("connect to the internet"; on a product of 5 offline days or fewer that shows from the first day offline) or the licence expires soon. `test` is a test licence (any number of computers, for trying the application): the application may say so, or limit what it does |
| `NeedsActivation` | No licence on this computer | Asks for a key, with the **How to get a key** link |
| `InvalidKey` | Malformed, unknown, or another product's key | Asks again, with the **How to get a key** link |
| `AlreadyActivatedElsewhere` | The key is in use on another computer | Explains how to ask for a reset |
| `Revoked` | Revoked, or this computer was reset | Locks; shows the support contact. The local activation is cleared, so a new key can be entered; shows the **How to get a key** link |
| `Expired { at }` | The licence expired | Locks; shows the support contact and the **How to get a key** link. Checked again daily, so an extended licence unlocks by itself |
| `ProductUnavailable` | The product was deactivated | Locks. Checked again daily |
| `NeedsCheck` | The product's offline days without a check (30 by default), or the clock was set back | Asks to connect; unlocks after the next successful check |
| `RateLimited { retry_after }` | Too many attempts | Asks to wait |
| `Unreachable` | Activating needs the server, which can't be reached | Asks to try again later |
| `ApplicationPending` | A licence application from this computer waits for the administrator | Says so; may offer to type a key instead |
| `ApplicationRejected { reason }` | The application was rejected (or its licence withdrawn) | Shows `reason` (the administrator's, for the applicant); offers to apply again or to type a key, with the **How to get a key** link |

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

## Without the crate: another language

An application that can't use the Rust crate does the same work itself, over the
[application API](api.md#application-api). This is what `clavren-client` does, step by step;
copying it keeps the same guarantees.

Bounty Board's Python port (`bounty_board/application/licence.py`, with the protocol rules in
`domain/licence.py` and the adapters in `adapters/clavren/`) is a complete example. It keeps the
rules apart from the network, the disk and the crypto, so its tests can swap each one out.

### Encoding

- Binary values are unpadded base64url. Refuse padded input, and refuse values of the wrong
  length: 32 bytes for keys and nonces, 64 for signatures.
- Key normalization drops spaces, tabs and dashes, upper-cases the rest, and reads `I` and `L` as
  `1` and `O` as `0`. Anything outside Crockford's base 32 (`0-9`, `A-Z` without `I L O U`)
  refuses the key. What remains must be `BRN` and exactly 16 characters.
- Claims: `iat`, `exp` and `lic_exp` are integers (refuse a boolean or a string), `lic_exp` may be
  absent or `null`, and `test` counts only when it is exactly `true`.

### What to keep

Keep it in one file for each product and computer user, protected by the operating system (DPAPI
on Windows, the keychain on macOS, the Secret Service on Linux). On Windows, use DPAPI in
CurrentUser scope with `clavren:<product code>` as the entropy, as the crate does, so that one
product's file can't stand in for another's. Replace the file whole on every save: write a
temporary file with a random name, then rename it over the old one. When several copies of the
application can run at once, also hold a lock while saving. A single-instance application
doesn't need one.

| Field | What it is |
|-------|------------|
| Device key | A 32-byte Ed25519 private key, made once from a secure random source and saved **before** the first activation request |
| Activation | The `activation` ID from the activation answer |
| Token | The latest token |
| Clock offset | The token's `iat` minus the system clock, in seconds, at the last successful answer |
| Latest seen, last seen, set back | The latest and the previous time the system clock showed, and how far it was set back in all since the last check |
| Last check | When the last answer came, by the system clock |
| Refusal | A definitive answer (revoked, expired, product unavailable), kept until a new activation. For an expired licence, also keep the last token's `lic_exp`, so the application can say when it ended |

Never log the key, the device key or the token. Keep them out of the state's debug output too.

When the file is loaded:

- **Missing:** start empty.
- **Damaged, or protected for another account or computer:** start empty. The application asks
  for its key again.
- **There but unreadable** (another program has it open): start empty, but never save over it.
  Every save fails until the application starts again, so a passing lock can't wipe out a
  licence.

Take a lock (a mutex) around every read and change of the state. Never hold it while waiting for
the network.

### Activating

The key prompt links **How to get a key** to `https://<server>/apply?product=<code>` (the code
percent-encoded, no doubled `/` after the server address), opened in the browser.

1. Normalize the typed key ([test vectors](api.md#test-vectors)). If it doesn't parse, ask again
   without sending it.
2. Create and save the device key if there is none. If it can't be saved, send nothing: the key
   would be bound to a device this computer can't prove it is.
3. `POST /api/v1/activations` with the product code, the canonical key, the device public key,
   the device name (cut to 200 characters, never empty), the platform and the application
   version. Use HTTPS only (plain `http://` only to this computer), a 15-second timeout, and a
   `User-Agent` such as `<application>/<version>`.
4. On `200` or `201`, save the `activation`, clear any refusal, and take the token (see
   [Taking a token](#taking-a-token)). A `200` is the same computer asking again, because the
   first answer was lost. If the token doesn't verify, still save the activation, with the last
   check set to now, so that the next check fetches a new token. If the activation can't be
   saved, report an error: trying again is safe, because the server knows this device key.
5. Otherwise, map the `error` to an outcome: `invalid_key`, `already_activated`,
   `licence_revoked`, `licence_expired`, `product_unavailable`, or `rate_limited` (wait the
   `Retry-After` seconds). Treat anything else, or an answer that isn't JSON (a proxy's page), as
   "can't be reached".

### Checking in

When the last check is more than 24 hours old, or there is no usable token:

1. `POST /api/v1/activations/{activation}/challenges` with no body.
2. Sign the [check message](api.md#post-apiv1activationsactivationchecks) with the device key.
3. `POST /api/v1/activations/{activation}/checks` with the challenge, the nonce, the signature
   and the application version.
4. On `200`, take the new token. If the saved activation changed while the check was running (the
   user activated a new key), throw the answer away.
5. On `licence_revoked` or `activation_revoked`, save the refusal, drop the token **and the
   activation**, so that a new key can be entered. On `licence_expired` or
   `product_unavailable`, save the refusal, drop the token, and keep checking daily, since the
   licence may be extended or the product turned back on. A refusal counts only once it is saved.
   If it can't be saved, nothing changes, as with any other failure.
6. Anything else (network, `5xx`, `rate_limited`, a malformed answer, a token that doesn't verify,
   a state that can't be saved) changes nothing. Retry after 1, 5, 15 and then every 60 minutes,
   or after `Retry-After` (60 seconds when it is missing) when that is longer. A success resets
   the backoff.

Run the check on a background thread or task. The UI never waits on the network. The checker:

- wakes at least every ten minutes, to notice a check that came due and to save the time seen
- tells the application only when the outcome's kind changes (a renewed token is not a change)
- survives its own bugs and the application's: an unexpected error is logged and retried after
  the longest backoff, and an error in the application's callback is logged while checking goes on
- stops at shutdown when asked: it wakes, lets a check under way finish (two requests of at most
  15 seconds each), and saves the time seen

### Taking a token

A new token is kept only if it verifies. Then, in this order:

1. Set the clock offset to the token's `iat` minus the system clock now. Use the signed `iat`,
   never the answer's unsigned `server_time`.
2. Set the last check, the latest seen and the last seen to now, and reset the set-back total to 0.
3. Verify the token as at start (below). If it fails, keep everything as it was.

### At start: offline

Without any network access:

1. If there's a refusal, show it.
2. If there's no token, the outcome is "needs activation" when there's no activation, and "needs
   a check" when there is one.
3. Add any step back since the last time seen to the set-back total. If the clock is more than an
   hour behind the latest time seen, or the total is over an hour, the outcome is "needs a
   check".
4. Verify the token at the server's time (the system clock plus the clock offset):
   - the header is `{"alg":"EdDSA","typ":"JWT"}`, and its `kid` is one of the embedded public
     keys
   - the Ed25519 signature over `<header>.<claims>` is valid (strict verification)
   - `aud` is the product code
   - `exp` is later than the server's time
   - `dev` is the BLAKE3 hash of the device public key, unpadded base64url
   - `act` is the saved activation

   Any failure means "needs a check".
5. Otherwise it's licensed until `exp`. `lic_exp` is the licence's own expiry, and `test` marks a
   test licence.

Record the time just looked at (step 3) on every status call, even when the licence is locked, so
that a clock set back while the application is locked can't simply be waited out. Save it now
and then: once it has moved ten minutes past the saved one, when the set-back total has changed,
and at shutdown. That way a clock set back between runs is noticed too.

Judge how close `exp` and `lic_exp` are by the server's time, not the system clock, for example
when warning that `exp` is less than 5 days away.

### Checking the port

- Port the [test vectors](api.md#test-vectors) as unit tests: key normalization, the check
  message, and the device hash.
- Test the rules against an in-memory server that signs real Ed25519 tokens with a key the tests
  trust, and a clock the tests move. Cover activating, checking, every refusal, a token for
  another device or activation, the offline days run out, and a clock set back at once and a little at a
  time.
- Run a contract test against the real server with a [test licence](#test-licences) (see below).

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

### Test licences

Every product comes with a test licence, whose key is shown on the dashboard's Products page. It
works on any number of computers at once, so one key serves every developer and CI machine:

- **Live contract test:** activate the test key against the real server, verify the token with
  the public keys the application ships, then run a signed check. Keep the state in memory, never
  in the application's data folder. Leave the test out of the default run, since it needs the
  network, and skip it when the key isn't set.
- **Never write a key in the repository.** Read it from an environment variable, for example
  `<APP>_LICENCE_TEST_KEY`.
- **Name the device** something like `<computer> (live test)`, so that its activations are easy
  to spot on the dashboard. Each run adds one.
- **Keep the application's own tests licensed** by default, against the in-memory server, with
  one switch (an environment variable) to start them unlicensed for the licence screen's tests.

---

## In the application

- **Lock everything behind the licence, not just the window.** Background work (scheduled jobs,
  alerts, lookups) also asks `status()` first and does nothing until it is `Licensed`. The
  licence screen comes up even when the application starts hidden (in the tray, say), because
  its work has stopped.
- **Show the licence in the settings:** whether it is a test licence, whether it is permanent or
  until when, and until when it works offline, highlighted when that is under 5 days away. Offer
  **Check now**, which runs a check off the UI thread.
- **Stop the checker at shutdown,** before closing anything its check uses (the HTTP client, for
  one), on every way the application quits.
- **Tell users what is sent.** The key when activating, then a signed check once a day, which
  carries the computer's name and the application version. A privacy section or user guide
  should say so, name the licence server, and say that the saved licence file is encrypted for
  this account and useless when copied.
