[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/02 14:18:55.155237 )
[//]: # (+ Revised: 	2026/11/03 08:25:53.123975 )
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

The key is written to a temporary file and renamed into place, so a crash can't leave half a key,
and an existing key is never overwritten.

## Licence state is derived, not stored

A licence has no `status` and no `redeemed_at` column. Both would copy facts the database already
holds, which is what BCNF rules out:

- **revoked** when `licenses.revoked_at` is set;
- **redeemed** when it has an activation with `revoked_at IS NULL`;
- **unused** otherwise.

The redemption time is that activation's `activated_at`. The `license_states` view computes the
state for listings and filters.

Concurrent activations, resets and revocations of the same licence are serialized by locking the
licence row (`SELECT … FOR UPDATE`). The partial unique index on active activations is the second
line of defence.

## Revoking a device is a reset

The dashboard can revoke an activation from the Activations page, and it is the same operation as
resetting its licence. A separate "unbound but still locked" state would need its own stored flag
and gives nothing a licence revocation doesn't already give.

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

## The dashboard's tests run against a frozen clock

The E2E and screenshot tests drive a real server (`examples/e2e_server.rs`) whose clock stands
still, so every date on screen is the same in every run and the screenshot baselines never drift.
A frozen clock leaves three authenticator codes (the frozen step and either side) plus the ten
recovery codes; the tests spend them carefully (`web/admin/e2e/credentials.ts`).

## Normalization (BCNF) check

Each table's non-trivial functional dependencies have its key as the determinant:

| Table | Key | Notes |
|-------|-----|-------|
| `products` | `id`; `code` | Both candidate keys; the rest depends on either |
| `customers` | `id`; `email` when present | Name and email live here, not on each licence |
| `licenses` | `id`; `key_lookup` | No stored state or redemption time (derived, see above) |
| `license_key_secrets` | `license_id` | |
| `activations` | `id` | At most one row per licence with `revoked_at IS NULL` (partial unique index) |
| `admin_users` | `id`; `username` | |
| `admin_recovery_codes` | `id` | |
| `admin_sessions` | `id`; `token_hash` | |
| `admin_preferences` | `admin_user_id` | `preferences` is one opaque document per admin |
| `audit_logs` | `id` | `metadata` is a snapshot of the event, not a copy of live data |
| `verification_challenges` | `id` | |
| `backup_runs` | `id` | |
| `check_counts` | `hour` | One total per hour, not a copy of anything stored elsewhere |

A schema test checks these rules once the migrations exist.
