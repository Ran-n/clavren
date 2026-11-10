[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/02 14:18:55.155237 )
[//]: # (+ Revised: 	2026/11/10 17:59:23.782152 )
[//]: # ( ---------------------------------------------------------------------- )

# Design decisions

Why Clavren is built the way it is, where it departs from the usual defaults, and the checks
behind its database design. The full specification is [spec.md](spec.md).

---

## Stack: Rust and React instead of Python and Flet

The default stack for new projects is Python (FastAPI, SQLAlchemy, Alembic) with a Flet frontend
in separate repositories. Clavren departs from it:

- **One binary on a Raspberry Pi.** The server, the admin CLI and the compiled dashboard ship as
  a single file run by systemd. No Python environment to install or keep up to date on the Pi.
- **A Rust client crate.** avalio-lite is a Rust application. The protocol crate is shared by the
  server and the client, so the two can't disagree on a format.
- **React for the dashboard**, embedded in the binary, keeps the admin UI same-origin with the
  API (simpler cookies and CSRF), with no second service to deploy.
- **One repository**, because server, client and dashboard change together whenever the protocol
  does.

## PostgreSQL on the same Raspberry Pi

At a few thousand licences SQLite would also do, but PostgreSQL was chosen and runs on the same
Pi, installed from the OS packages and listening on localhost only. Backups use `pg_dump`.

## Checking in: once a day, 30 days offline

Applications check with Clavren once a day: when their last successful check is more than 24
hours old, at start or while running. A revocation or expiry change therefore reaches an online
computer within about a day. This replaced an earlier "every start and every 4 hours" design: a
day's delay is acceptable for this use and it keeps traffic to the Pi minimal.

A live push connection (Server-Sent Events) was considered so running apps would lock within
seconds, and set aside in favour of the daily check. No design can reach a computer that is off
or offline; those find out at their next check.

Tokens last 30 days, so a computer on a blocked school network keeps working that long without a
check. Requiring internet at every start would lock out teachers whenever the school network or
the Pi is down.

Checks use a server challenge rather than a device timestamp, because school PCs often have the
wrong time and a timestamp-based check would fail on them.

## IPv6 clients are limited by their /64

Rate limits key an IPv6 client by its /64 network, not its full address: a connection usually gets
a whole /64 and can pick a new address for every request, which would make a per-address limit
meaningless. Computers on one IPv6 network (a school) then share one allowance, as they already do
over IPv4 behind one NAT address. The limits (10 activations, 120 checks per hour) are sized for
that: a school activates a handful of computers at a time, and each checks once a day.

## Tokens are judged by the server's clock

The client keeps how far the computer's clock was from the server's at its last answer, and judges
the cached token by the computer's clock plus that difference. The alternative, the computer's
clock alone with the latest time seen as a rollback guard, either locks out a computer whose clock
is simply wrong (one set ahead never runs) or, when the guard is reset after every check, lets a
clock set back before a check stretch the 30 days by as much as it was set back. With the
difference, a wrong clock works normally, a clock set back after a check is noticed against the
latest time seen (system-clock time) and asks for a check, and one set back before a check gains
nothing. The difference is measured from the token's signed issue time (`iat`), not from the
answer's `server_time`: that field isn't signed, so a proxy that changed it could otherwise shift
the clock by years. Steps back also add up since the last check, so setting the clock back 59
minutes every hour can't hold it inside the one-hour allowance for ever.

## Customers are recorded

The administrator hands out keys by hand and has to find a customer's licence when they ask for a
reset, so each licence can point to a customer (name, optional email, notes). Customers are a
table of their own: one customer can hold several licences, and copying name and email onto each
licence would break BCNF. Deleting a customer erases their personal data and leaves their
licences working; the audit log refers to customers by ID only, so nothing is left behind.

## No Docker

Clavren runs natively: PostgreSQL from the OS packages, Clavren as a systemd service, backups as
a systemd timer. Docker adds a layer to update and debug on a small machine without solving a
problem this single-instance service has.

## Hashing: BLAKE3 by default

Every hash, keyed hash and key derivation uses BLAKE3: keyed BLAKE3 for licence-key lookup,
`blake3::derive_key` for subkeys, plain BLAKE3 for fingerprints, token, nonce and recovery-code
hashes, and backup checksums. The only exceptions:

| Where | Function | Why not BLAKE3 |
|-------|----------|----------------|
| Admin passwords | Argon2id | Human passwords need a deliberately slow, memory-hard function |
| TOTP codes | HMAC-SHA-1 | Fixed by RFC 6238; authenticator apps only support it |
| Ed25519 signatures | SHA-512 (internal) | Part of the signature algorithm |

## A missing master key is only replaced on an empty database

Clavren creates `master.key` on first start, but refuses to start when the file is missing and
the database holds licences or admin accounts: stored keys, lookup hashes and TOTP secrets all
depend on it, and a new key would silently make them unreadable. The spec first named only
licences; admin accounts were added because their TOTP secrets are encrypted with it too.

The key is written whole to a temporary file of its own, then hard-linked to `master.key`, which
fails rather than replace a file already there: a crash can't leave half a key, an existing key is
never overwritten, and when the server and `clavren-admin` start together both use the one key that
landed. A file system without hard links gets the key created in place, still exclusively; a crash
there can leave a short key, which is refused on read.

## Licence state is derived, not stored

A licence has no `status` and no `redeemed_at` column. Both would copy facts the database already
holds, which is what BCNF rules out:

- **revoked** when `licenses.revoked_at` is set;
- **redeemed** when it has an activation with `revoked_at IS NULL`;
- **unused** otherwise.

The redemption time is that activation's `activated_at`. The `license_states` view computes the
state for listings and filters.

Concurrent activations, resets and revocations of the same licence are serialized by locking the
licence row (`SELECT … FOR UPDATE`). A trigger on `activations` is the second line of defence: it
refuses a second active activation of a licence that isn't a test licence. It locks the licence row
itself, so it holds even for a write that skipped the application's lock.

## Test licences

A test licence is an ordinary licence with `is_test` set: the same key format, storage, reveal,
revoke and delete. What differs is small and stays in the server: it may be active on any number
of computers, its tokens say `"test": true`, and the Overview leaves it, its computers and its
checks out (a test check isn't added to `check_counts`). Applications decide what a test licence
means for them (a banner, fewer features) from the token; Clavren doesn't.

- **No limit on computers.** A test licence is the administrator's own, handed to developers and
  testers; a cap would be one more setting to manage. A leaked test key is revoked like any other.
- **A trigger instead of the unique index.** One-active-computer used to be a partial unique index
  on `activations (license_id) WHERE revoked_at IS NULL`. An index can't look at the licence, so a
  trigger keeps the rule for licences that aren't test licences. Copying `is_test` onto each
  activation would let the index stay, but would store a fact of the licence twice.
- **`is_test` never changes** (a trigger refuses it). Turning a test licence into a sold one while it
  is on three computers would break the one-computer rule; issuing a new licence is clearer.
- **One comes with every new product**, issued in the transaction that creates it, so a new
  application can be tried at once. The API takes `"test_licence": false` for none.
- **Ending one computer ends only that one.** Revoking an activation from the Activations page used
  to reset the whole licence; for a licence with one computer that is the same, and for a test
  licence it leaves the others working. Reset still ends every computer.
- Undoing the migration keeps test licences as ordinary ones, each on its latest computer only.

## Revoking a device is a reset

The dashboard can revoke an activation from the Activations page, and it is the same operation as
resetting its licence (for a test licence, only that computer's activation ends). A separate "unbound but still locked" state would need its own stored flag
and gives nothing a licence revocation doesn't already give.

## Licences can be deleted

The user decided (2026-11-03) that any licence can be deleted, revoked or not, so test keys and
mistakes don't stay in the table and in exports for ever. Deleting removes the licence, its
encrypted key and its activations in one transaction; `activations.license_id` stays `ON DELETE
RESTRICT`, so history can only disappear through that one function. Revoking is still the way to
stop a key while keeping its record: a revoked key's hash stays and is refused as revoked, while a
deleted key reads as unknown. The audit log keeps its entries for a deleted licence, which name it
by ID only and say what state it was in, so the log still shows whether a live key was cut off.

## Deleting a product takes its licences

The user decided (2026-11-03) that deleting a product always works and takes every licence of it
with it, live ones included; deactivating remains the way to pause a product without losing
anything. Unlike a bulk licence delete (at most 1,000, chosen on screen), this has no limit and
reaches licences no filter on screen shows, so the dashboard asks for the product's code to be
typed before it confirms, with the counts from the server in the warning. Customers stay: they may
have other products' licences.

## Product codes made from names

A new product's code is optional; without one the server makes it from the name (accents dropped,
lowercase, dashes, `-2`, `-3`, … when taken). The server makes it, not the dashboard, because only
the server knows which codes are taken, and a copy of the rules in TypeScript would drift. A typed
code that is taken is refused rather than suffixed: typing it was a choice. Renaming never changes
the code, because every token issued for the product carries it.

## The encrypted key lives in its own table

`license_key_secrets` holds the encrypted key, nonce and encryption version, one row per licence.
A revoked licence's key is gone because its row is deleted, rather than being two NULL columns
with a CHECK constraint keeping them in step.

## Changing the signing key is a setting, not a command

Signing keys are derived from the master key by version (`v1`, `v2`…), so there is nothing to
generate or store: the server signs with the version in `CLAVREN_SIGNING_KEY_VERSION`, and
`clavren-admin show-public-key` prints the key in use and the next one. An earlier draft had a
`rotate-signing-key` command; it would only have edited the configuration file, and switching
before every application trusts the new key would lock customers out, so the switch is a
deliberate edit and restart ([deployment.md](deployment.md#changing-the-token-signing-key)).

## The check repeats the challenge's nonce

The server keeps only a BLAKE3 hash of each challenge's nonce, so a leaked database row can't be
used to answer a challenge. To verify the device's signature it needs the nonce itself, so the
check request sends it back and the server compares its hash.

## Checks are counted by the hour

The Overview shows both the computers that checked in during the last 24 hours (from each
activation's last check) and the number of checks made. Checks are counted in `check_counts`, one
row per hour with its total, rather than one row per check: the count is all the Overview needs,
and a row per check would grow by one row per computer per day for nothing else. Hours older than
30 days are deleted by the hourly clean-up. The 24-hour total is the current hour plus the 23
before it.

## Expanding inside an expanded record

`?expand=licence.customer` on the activations list expands the customer inside the expanded
licence, still one query per field. The Activations page needs the product and customer of every
computer, and the alternative, copying them onto each activation, would break the rule against
convenience copies.

## Remembered searches

Preferences remember each table's search, so a page comes back as the admin left it. A search
can name a customer, which is accepted; a licence key never is: the server refuses any search or
filter value that reads as a key, and the dashboard never sends one. Customer filters are stored
as IDs.

## The dashboard is embedded by `build.rs`

`build.rs` turns `web/admin/dist` into a table of `include_bytes!` entries; no crate is needed for
that. Without a build the server still compiles and serves the API, and the dashboard path shows
how to build it. `index.html` is never cached (it names the build's files); the hashed files under
`assets/` are cached for good. Pages get a strict Content-Security-Policy: no inline scripts,
nothing from elsewhere, no framing.

## Updates are found and installed outside the server

The server makes no outgoing connection and runs as an unprivileged user under
`NoNewPrivileges` and `ProtectSystem=strict`: it can't replace its own binary or restart itself,
and must not be able to, or a stolen admin session would own the Pi. So two scripts do the work,
each started by a systemd path unit when the server writes a file in `/var/lib/clavren/updates`,
the only folder it can write:

- **Checking** (`clavren-update-check.sh`, as the clavren user) runs from a timer every 6 hours
  and when **Check now** writes `check-request`. It writes `latest.json`, which the server only
  reads. Same split as backups: a script the system runs, a server that reports.
- **Installing** (`clavren-update.sh`, as root) runs when **Update** writes `request.json`. The
  request is untrusted (the unprivileged server wrote it): it may only name a version, three plain
  numbers, and whether to skip the backup, both checked before use and never put in a command
  unchecked. Everything else is decided by the root script: it asks the forge again and requires
  the version to be the newest release right now and newer than the installed one, takes the
  bundle and checksum from that release, checks the BLAKE3 sum, and runs the bundle's own
  `install.sh`, so a release's new install steps apply as in an upgrade by hand.

The alternatives give the server too much. `sudo` for one command, a polkit rule allowing it to
start a unit, or running the server as root all let a compromised server choose what runs as
root; a request file lets it ask for one thing, which root checks.

Root never writes in the clavren-owned folder (the clavren user could plant a link there that
root would follow): it moves the request aside by name and writes its progress to
`/var/lib/clavren-update/status.json`, in a folder only root can write. The server reads both and
treats an update still "running" after 20 minutes (the service times out at 15) as failed.

A BLAKE3 checksum next to the bundle only catches a broken download, not a compromised forge
account. Signing releases is a separate step, not taken yet.

## The dashboard's tests run against a frozen clock

The E2E and screenshot tests drive a real server (`examples/e2e_server.rs`) whose clock stands
still, so every date on screen is the same in every run and the screenshot baselines never drift.
A frozen clock leaves three authenticator codes (the frozen step and either side) plus the ten
recovery codes; the tests spend them carefully (`web/admin/e2e/credentials.ts`).

## Limits accepted at this size

Two weaknesses found in the 2026-11-08 audit are left as they are, because Clavren is sized for
hundreds to a few thousand licences ([spec.md](spec.md)) and fixing them costs more than they
risk at that size. Revisit them if that changes.

- **Deleting a huge product.** Deleting a product removes all its licences and their activations
  in one transaction (`crates/clavren-server/src/products/mod.rs`), and every request is cut off
  after 30 seconds (`crates/clavren-server/src/http/mod.rs`). Somewhere around 100,000 licences
  on a Raspberry Pi the delete would take longer, roll back, and answer `408` every time. A few
  thousand licences delete in well under a second. The fix, if ever needed: delete in batches in
  the background, or mark the product deleted and let the hourly clean-up remove its rows.
- **Using up an activation's checks.** The limit of 30 checks per hour per activation is counted
  once a challenge made for that activation is used, before the device's signature is checked
  (`crates/clavren-server/src/activations/checks.rs`; made-up activation IDs never get that far,
  so they cost no memory), so anyone who knows an activation's ID can spend them and keep that computer from checking for
  an hour. The computer keeps working on its cached token (30 days), and the ID only exists on the
  computer itself and in the dashboard, so it can't be guessed. The fix, if ever needed: count only
  failed checks (wrong challenge or signature) against the activation.

## Normalization (BCNF) check

Each table's non-trivial functional dependencies have its key as the determinant:

| Table | Key | Notes |
|-------|-----|-------|
| `products` | `id`; `code` | Both candidate keys; the rest depends on either |
| `customers` | `id`; `email` when present | Name and email live here, not on each licence |
| `licenses` | `id`; `key_lookup` | No stored state or redemption time (derived, see above); `is_test` depends on the licence alone |
| `license_key_secrets` | `license_id` | |
| `activations` | `id` | At most one row per licence with `revoked_at IS NULL`, unless it is a test licence (trigger) |
| `admin_users` | `id`; `username` | |
| `admin_recovery_codes` | `id` | |
| `admin_sessions` | `id`; `token_hash` | |
| `admin_preferences` | `admin_user_id` | `preferences` is one opaque document per admin |
| `audit_logs` | `id` | `metadata` is a snapshot of the event, not a copy of live data |
| `verification_challenges` | `id` | |
| `backup_runs` | `id` | |
| `check_counts` | `hour` | One total per hour, not a copy of anything stored elsewhere |

A schema test checks these rules once the migrations exist.
