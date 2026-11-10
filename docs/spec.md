[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/02 14:18:55.098038 )
[//]: # (+ Revised: 	2026/11/10 17:59:24.066867 )
[//]: # ( ---------------------------------------------------------------------- )

# Clavren — Specification

Why each decision was made is in [design-decisions.md](design-decisions.md).

---

## 1. What Clavren is

Clavren is the licensing server for Breren's own applications (avalio-lite first, others later). An application asks for a licence key when it is installed; Clavren says whether that key is valid, ties it to that computer, and keeps checking it whenever the computer is online.

- The administrator hands out keys **by hand** (e.g. by email after a sale), and records who received each one.
- Licences are **permanent** or **time-limited** (work until a date, which can be extended).
- A key works on **one computer (one Windows user account) at a time**. The administrator can **reset** it so the customer can move to a new computer, or **revoke** it for good.
- An application checks its licence with Clavren **once a day**: whenever its last successful check is more than 24 hours old, at start or while running, if it has internet. A revocation or expiry change reaches an online computer within about a day. A computer without internet keeps working for **up to 30 days**, then needs to check in.
- Expected size: hundreds to a few thousand licences, a handful of products, one administrator at first (more later).
- It runs on a **Raspberry Pi 5**, natively (no Docker), with PostgreSQL on the same Pi, reachable from the internet through Cloudflare Tunnel.

### Out of scope

- Selling: payments, shops, carts, subscriptions billing, shop webhooks. Sales happen elsewhere; the administrator issues keys by hand.
- Sending email: the dashboard prepares the message, the administrator sends it.
- Customer accounts or any page customers log into. Customers only ever type a key into an app.
- Trials for customers and feature tiers (editions). Can be added later. (Test licences, for the
  administrator's own testing, are in scope: section 3.)
- Stopping people who patch the application binary to skip the check. Clavren prevents key sharing and casual piracy, not cracking; nothing is added that only aims at that.

---

## 2. How it is used

### Issuing a key

1. A customer buys or is granted a product.
2. In the dashboard the administrator clicks **Issue licences**, picks the product, the customer (an existing one, or a new name and email typed in place), how many, and optionally an expiry date.
3. Clavren generates the keys, e.g. `BRN-7KPX-4M9D-QW2R-8NFC`, stores them encrypted and shows them.
4. **Copy message** puts a ready-to-send text on the clipboard (the product's message template with the customer's name and key filled in). The administrator pastes it into an email.

### Activating

1. The customer installs the application and types the key.
2. The application creates a device key pair (protected by Windows), and sends the licence key and the device's public key to Clavren.
3. Clavren checks the key belongs to that product, isn't revoked or expired, and isn't in use on another computer. It records the activation and returns a signed token.
4. The application stores the token and works.

### Once a day

1. The application starts immediately if its stored token is valid (checked offline: signature, product, expiry, this device).
2. If its last successful check is more than 24 hours old (at start, or later while it keeps running), it asks Clavren to confirm in the background. Clavren checks the licence is still valid and returns a fresh token (valid 30 days, or until the licence's expiry if sooner).
3. If Clavren says the licence was revoked, reset or has expired, the application locks with a clear message within seconds.
4. If Clavren can't be reached, nothing changes: the application keeps running on its token and tries again later.

### Moving to a new computer

1. The customer writes: "I have a new PC". The administrator finds them by name or email.
2. **Reset** on their licence. The old computer is locked at its next check; the same key now activates on the new computer.

### Revoking

**Revoke** on a licence (used or not) makes it permanently unusable. A computer using it locks at its next daily check (within about a day if it goes online); an offline one at the latest when its token expires (30 days). The confirmation strip in the dashboard says this.

### Changing the expiry

The administrator edits the expiry date in place (extend a school year, make it permanent, end it early). Online computers pick it up at their next check.

---

## 3. Concepts

- **Product**: an application that uses Clavren. It has a unique `code` (e.g. `avalio-lite`, used in tokens), made from the name when none is given, a name, a description, an active flag, and the message template for issued keys. Deleting a product deletes all its licences and their history; their computers lock at their next check. Deactivating keeps everything.
- **Customer**: who received a key: name, optional email, notes. Only the administrator sees it. One customer can have several licences.
- **Licence**: one key for one product, optionally for a customer, optionally with an expiry date. Its state is **unused**, **redeemed** (in use on a computer) or **revoked**; the state is derived, never stored (section 7).
- **Test licence**: a licence for trying an application against the real server, not a sale. It works on **any number of computers at once**, its tokens say it is a test licence (the application may show so), and the Overview leaves it, its computers and its checks out. Every new product comes with one (the product's creation shows its key); more are issued with **Issue licences** by ticking **Test licences**. A licence is a test licence from issuing to deletion.
- **Activation**: one computer (Windows user account) a licence was bound to: its public key, name, platform, app version, when it was activated and last checked, and whether and why it was revoked. A licence has at most one active activation (a test licence any number); earlier ones stay as history.

---

## 4. Licence keys

Format: `BRN-7KPX-4M9D-QW2R-8NFC`: fixed prefix `BRN` and 16 Crockford Base32 characters in four groups. Alphabet `0123456789ABCDEFGHJKMNPQRSTVWXYZ` (no I, L, O, U). 16 characters carry 80 random bits, so guessing a key is infeasible; the prefix only identifies the format.

Generation, per key:
1. 10 random bytes from the operating system's cryptographic RNG.
2. Encode as 16 Crockford Base32 characters; add prefix and dashes.
3. Lookup hash: keyed BLAKE3 of the canonical key (prefix + 16 characters, uppercase, no dashes).
4. Encrypt the key with XChaCha20-Poly1305 (fresh 24-byte nonce; licence ID and product ID as additional authenticated data).
5. Store the lookup hash (unique) and the encrypted key. On a lookup-hash collision, generate again.

Normalization of a typed key: accept any letter case, ignore spaces and dashes, map the Crockford look-alikes `I`/`L` → `1` and `O` → `0`, then require the prefix and exactly 16 characters of the alphabet. The same function (in `clavren-protocol`) runs in the application, to reject a mistyped key before going online, and on the server.

The encrypted key is kept while the licence is unused or redeemed, so the administrator can show it again (e.g. after a reset, for a customer who lost it). Revoking deletes it; the lookup hash stays so the key can never be accepted again. Any licence can be deleted, with its activations; its key then reads as unknown and a computer using it locks at its next check.

---

## 5. The licence check, as the application sees it

The `clavren-client` crate does all of this for Rust applications; others follow the HTTP protocol in section 6.

### Device identity

- On first activation the client generates an Ed25519 key pair. The private key is protected with Windows DPAPI in **CurrentUser** scope and stored under the user's `%APPDATA%`, with the activation ID, the cached token and the latest time seen.
- A licence therefore belongs to one Windows user account on one PC. On a shared PC each account needs its own licence.
- An optional hardware fingerprint (hashed with BLAKE3 on the device; raw serial numbers never leave it) is sent for diagnostics only, never trusted as proof.
- Device storage is behind a trait so macOS Keychain and Linux secret storage can be added later.

### When the client checks

- **At start**: validate the cached token offline (signature with the embedded public key, `aud` = this product, not expired, the token's `dev` claim matches this device's key, no clock rollback). Valid → the application starts at once. If the last successful check is more than 24 hours old, check online in the background.
- **While running**: check online once 24 hours have passed since the last successful check. The interval is configurable in the client.
- **Always online first**: without a valid cached token (first activation, token expired, clock rolled back) the application checks at once, whatever the interval.
- **After a network failure**: retry with backoff (1, 5, 15, 60 minutes), never more often.
- Checks run off the UI thread; the application is never blocked waiting for the network.

### What the client reports

The application receives one typed outcome and shows its own translated message:

| Outcome | Meaning | Application does |
|---|---|---|
| `Licensed { licence_expires_at, offline_until, test }` | Valid | Runs. May show a notice when `offline_until` is under 5 days away ("connect to the internet to keep using…"), and that it runs on a test licence when `test` |
| `NeedsActivation` | No licence on this computer | Asks for a key |
| `InvalidKey` | Malformed, unknown, or for another product | Asks again |
| `AlreadyActivatedElsewhere` | Key in use on another computer | Shows the support contact (to ask for a reset) |
| `Revoked` | Licence revoked, or this computer was reset | Locks; shows the support contact |
| `Expired { at }` | Licence past its expiry date | Locks; shows the support contact |
| `ProductUnavailable` | Product deactivated | Locks |
| `NeedsCheck` | Token expired (30 days offline) or clock rolled back | Asks to connect to the internet |
| `RateLimited { retry_after }` | Too many attempts | Waits |

**Definitive** refusals (`Revoked`, `Expired`, `ProductUnavailable`, unknown activation) delete the cached token and lock immediately. **Temporary** failures (no network, timeout, server error, rate limit) change nothing while the token is valid.

### Clock rollback

The client keeps the latest system-clock time it has seen and, at each successful answer, the difference between the server's time (the signed token's `iat`, never the unsigned `server_time`) and its own; it judges the cached token by its clock plus that difference. If the system clock is ever more than 1 hour earlier than the latest time seen, or was set back more than 1 hour in all since the last successful check (small steps back add up, across restarts too), the cached token is treated as invalid until an online check succeeds. A wrong clock therefore works normally, and a clock set back before a check gains nothing.

### Never

The client never logs or displays the licence key, the token or the private key.

---

## 6. Public HTTP API (used by applications)

All under `/api/v1`, JSON over HTTPS. Every response includes `server_time`. Binary values are unpadded base64url; IDs are lowercase hyphenated UUIDs.

### `POST /api/v1/activations`

Request: `product` (code), `key`, `device_public_key` (32-byte raw Ed25519), `device_fingerprint` (optional), `device_name`, `platform`, `app_version`.

In one transaction: normalize the key, compute its lookup hash, find the licence and lock its row (`SELECT … FOR UPDATE`), check it belongs to that product, the product is active, the licence isn't revoked or expired and has no active activation (a test licence may have any number); create the activation.

- **201** `{ activation, token, token_expires_at, licence_expires_at, server_time }`.
- **Same device retrying** (the response was lost): if the active activation has this exact device public key, return it with a fresh token (200) instead of refusing.
- Errors: `invalid_key`, `already_activated`, `licence_revoked`, `licence_expired`, `product_unavailable`.

### `POST /api/v1/activations/{activation}/challenges`

Returns `{ challenge, nonce, expires_at }`: a random 32-byte nonce valid for 2 minutes. Only its BLAKE3 hash is stored. A challenge proves the request comes from the device holding the private key, without depending on the device's clock (school PCs often have the wrong time).

### `POST /api/v1/activations/{activation}/checks`

Request: `challenge`, `signature`, `app_version`. The signature is Ed25519 over the UTF-8 bytes of:

```
clavren-check-v1
<activation id>
<challenge id>
<nonce>
```

(lines joined with `\n`, no trailing newline). Clavren verifies the signature with the stored device public key, consumes the challenge atomically (a challenge works once), checks the activation isn't revoked, the licence isn't revoked or expired, and the product is active; updates `last_checked_at` and `app_version`.

- **200** `{ token, token_expires_at, licence_expires_at, server_time }`.
- Errors: `activation_revoked` (reset or revoked), `licence_revoked`, `licence_expired`, `product_unavailable`, `challenge_invalid`.

### Activation token

A JWT signed with Ed25519 (EdDSA), header `kid`. Claims: `iss` (public service URL), `aud` (product code), `sub` (licence ID), `act` (activation ID), `dev` (BLAKE3 of the device public key, base64url), `lic_exp` (licence expiry, if any), `test` (`true` for a test licence, absent otherwise), `iat`, `exp`, `jti`. `exp` = `min(now + 30 days, licence expiry)`; the 30 days are configurable.

Applications embed a **list** of trusted public keys by `kid`. Each signing key is derived from the master key with a versioned context (`v1`, `v2`…). Rotation: ship application versions that trust both keys, then switch the server with a CLI command once old versions are retired.

### Errors

Every error is `{ "error": "<code>", "message": "<English, for logs>" }`; 429 adds `Retry-After`. Codes: `invalid_key`, `already_activated`, `licence_revoked`, `licence_expired`, `product_unavailable`, `activation_revoked`, `challenge_invalid`, `rate_limited`, `invalid_request`, `internal_error`. An unknown key and a key for another product share `invalid_key`, so the server never reveals that a key exists for another product. Other refusals say why, because "already in use" is exactly when the customer needs to ask for a reset, and 80-bit keys with rate limits make guessing pointless.

### Limits

- Rate limits per client IP: activations 10/hour; challenges and checks 120/hour. Per activation: 30 checks/hour. Counted in memory (single instance).
- Request bodies up to 16 KiB; text fields up to 200 characters.
- No CORS (desktop applications don't need it).

`docs/api.md` documents every endpoint with examples, and **test vectors** for key normalization, the signed check message and the device fingerprint, so a client in another language can be verified against them.

---

## 7. Data model (PostgreSQL)

SQL migrations embedded in the binary and run at startup. All timestamps `TIMESTAMPTZ` in UTC. Every table is in BCNF; the check per table is in `docs/design-decisions.md`.

### `products`
`id` UUID PK · `code` text unique (immutable once it has licences, because tokens carry it) · `name` · `description` nullable · `message_template` text (placeholders `{customer_name}`, `{product_name}`, `{key}`) · `is_active` boolean · `created_at` · `updated_at`.

### `customers`
`id` UUID PK · `name` text · `email` nullable text, unique case-insensitively when present · `notes` nullable · `created_at` · `updated_at`.

Deleting a customer erases their personal data: their licences keep working and lose the link (`customer_id` set to NULL). Audit entries reference customers by ID only, never by name or email, so deletion really removes them.

### `licenses`
`id` UUID PK · `product_id` FK · `customer_id` nullable FK · `key_lookup` BYTEA unique (keyed BLAKE3) · `created_at` · `expires_at` nullable (NULL = permanent) · `revoked_at` nullable · `revocation_reason` nullable · `internal_notes` nullable · `is_test` boolean (never changes once issued).

There is no stored state and no redemption time; both would copy facts held elsewhere. The view `license_states` derives the state: **revoked** if `revoked_at` is set, else **redeemed** if it has an activation with `revoked_at IS NULL`, else **unused**. The redemption time is that activation's `activated_at` (the latest one's, for a test licence on several computers), and `active_computers` counts them.

### `license_key_secrets`
`license_id` UUID PK/FK · `ciphertext` · `nonce` · `encryption_version`. Deleted when the licence is revoked.

### `activations`
`id` UUID PK · `license_id` FK · `device_public_key` BYTEA · `device_fingerprint_hash` nullable BYTEA · `device_name` · `platform` · `app_version` (last reported) · `activated_at` · `last_checked_at` nullable · `revoked_at` nullable · `revocation_reason` nullable (`reset` or `licence_revoked`).

A trigger keeps at most one active activation per licence, except test licences (it locks the licence row, as activating does); revoked ones stay as history.

### `verification_challenges`
`id` UUID PK · `activation_id` FK · `nonce_hash` BYTEA · `created_at` · `expires_at` · `consumed_at` nullable.

### `admin_users`
`id` UUID PK · `username` unique · `password_hash` (Argon2id) · `totp_secret_ciphertext`, `totp_secret_nonce` (encrypted under their own derived key, admin ID as additional data) · `totp_last_used_step` nullable BIGINT · `created_at` · `last_login_at` nullable · `disabled_at` nullable.

### `admin_recovery_codes`
`id` UUID PK · `admin_user_id` FK · `code_hash` (BLAKE3; codes are 128-bit random) · `used_at` nullable.

### `admin_sessions`
`id` UUID PK · `admin_user_id` FK · `token_hash` BYTEA unique (BLAKE3) · `created_at` · `last_seen_at` · `expires_at` · `revoked_at` nullable. Raw session tokens are never stored.

### `admin_preferences`
`admin_user_id` UUID PK/FK · `preferences` JSONB (theme, per-table filters, search, sort, page size, visible columns, last page), validated against a versioned schema; never contains a licence key (a search or filter that reads as one is refused), while a remembered search may name a customer · `updated_at`.

### `audit_logs`
`id` UUID PK · `admin_user_id` nullable FK · `action` · `entity_type` · `entity_id` nullable UUID · `metadata` JSONB · `ip_address` nullable INET · `created_at`.

Recorded: licences issued, key revealed, copied as message or exported, licence edited (the names of the fields changed), reset, revoked, deleted (with the state it was in), activation created, refused checks (revoked, expired, unavailable), product and customer changes, admin login, failed login, logout, password and TOTP changes. Successful checks are not audited (they update `last_checked_at`). Kept indefinitely. Never contains keys, passwords, tokens, or customer names and emails.

### `backup_runs`
`id` UUID PK · `finished_at` · `archive_name` · `size_bytes` BIGINT.

### `check_counts`
`hour` TIMESTAMPTZ PK (a whole hour) · `checks` BIGINT (> 0): successful checks in that hour, for the Overview (test licences' checks aren't counted). One row per hour, not per check.

### Housekeeping
An hourly background task deletes used or expired challenges, expired or revoked sessions, and check counts older than 30 days.

---

## 8. Admin dashboard

A polished, responsive web application (React, TypeScript, Vite) embedded in the server binary, for administrators only. English only.

### Interaction rules (every page)

- **Inline editing**: click a value (product name, description, message template, customer name, email, notes, licence customer, expiry, notes and revocation reason), edit, Enter or leaving the field saves, Escape cancels. No edit dialogs or edit pages.
- **Inline creation**: a new product or customer is a new editable row at the top of its table; a new customer can also be typed straight into the **Issue licences** panel.
- **No modal dialogs**: a destructive action (revoke, reset, deactivate, delete licence, delete product, delete customer) turns its button into an inline confirmation strip: what will happen in plain words, an optional reason, a red **Confirm** and **Cancel**. Escape cancels. Deleting a product that has licences also needs its code typed before **Confirm** is enabled.
- **Disabled actions look disabled**: dimmed background and label, with a tooltip saying why (e.g. Reset on a licence that isn't in use).
- **One primary action per page**, large and on its own line (e.g. **Issue licences**); secondary actions smaller.
- **Everything is remembered**: theme, filters, search, sort, page size, visible columns and last page are saved in `admin_preferences` and restored at the next login, on any browser. Defaults (theme follows the system) apply until the admin picks something.
- **Selection and bulk actions**: row checkboxes and "select all matching the filter"; bulk actions use bulk endpoints.
- **Never frozen, never silent**: requests run in the background with a visible loading state; errors are shown in plain English with what to do. Raw codes, HTTP statuses and stack traces never reach the screen.
- **Light and dark themes**, switched with an icon button in the header that cycles System, Light and Dark.

### Pages

- **Overview**: products, licences by state, active computers, computers that checked in and checks made in the last 24 hours (all without test licences, which the page says), last successful backup (warning if over 48 hours old), and the server: its version, the last check for a newer release (or why it failed) and **Check now**.
- **Every page**: when a newer release is out, a notice with the version, **What's new** (the release page) and **Update to X.Y.Z**, which turns into an inline confirmation (with "Update without a backup" to tick when the backup drive isn't mounted), then shows the update's progress and, after the dashboard reloads itself, "Updated to X.Y.Z".
- **Licences**: table with key (hidden, **Reveal** / **Copy**), product, customer, state, issued, redeemed, last check, customer, expiry and notes (inline; revocation reason inline on revoked licences); the product, key, state and dates are read-only. Search by key (normalized and looked up by hash), customer name or email, licence ID. Filters by product, state, expiry (permanent, expiring within 30 days, expired), kind (test licences), each of which can also exclude ("Except revoked"), plus a hide box for key, customer or licence ID. Test licences carry a **Test** badge. Primary action **Issue licences**: inline panel with product, customer (picked from a searchable list of every customer, or typed as a new one), quantity (up to 1,000), optional expiry and notes, and a **Test licences** tick (never remembered); the new keys appear there with **Copy message** (one customer) and **Download CSV**. Bulk: set customer, edit expiry, edit notes, edit reason, export CSV, revoke, delete (with the server's count of what goes).
- **Licence detail**: all fields (a test licence also how many computers it is on), Reveal/Copy, **Copy message**, Reset (only when in use; every computer of a test licence), Revoke (unless revoked), Delete (any state), and the activation history (every computer it was on, with reasons).
- **Customers**: table (inline editing and creation), search by name or email, each with their licences; delete with inline confirmation ("their licences keep working; their name and email are erased").
- **Products**: table (inline editing and creation, the code made from the name when left empty; a new product's test licence key is shown with **Copy key** until the page is left), activate/deactivate (inline confirmation warning that every computer of that product locks at its next check), delete with its licences (the strip shows how many and how many are in use), link to its licences.
- **Activations**: table of computers: product, licence (with a **Test** badge), customer, device name, platform, app version, activated, last check, state; filters by state, product and kind. Revoking an activation here ends that computer's activation: the same as resetting its licence, except that a test licence's other computers go on.
- **Audit log**: who did what, when, from which IP; filters by action, admin, entity, date.
- **Account**: change password, re-enrol TOTP, regenerate recovery codes, sign out everywhere else.

### Keys in the browser

Responses containing a plaintext key carry `Cache-Control: no-store`. Keys are never written to browser storage, preferences, analytics or logs, and a revealed key is hidden again when the admin leaves the page. Reveal, copy-message and export are audited.

CSV export columns: `licence_id, product_code, customer_name, customer_email, key, state, issued_at, expires_at, notes, test`. Cells starting with `=`, `+`, `-`, `@`, tab or carriage return are prefixed with `'` so spreadsheets don't run them as formulas.

---

## 9. Admin API

Under `/api/admin`, authenticated by session cookie plus CSRF header; same-origin only. Every endpoint is documented in [api.md](api.md).

- **Expand**: related entities are IDs by default and full objects with `?expand=<field>` (repeatable). No `*_id` fields and no copied convenience fields. Licence: `product`, `customer`, `active_activation`. Activation: `license`. Audit entry: `admin_user`. Expanding a page runs one query per field, never one per row.
- **Pagination** on every list (cursor-based, page size up to 100).
- **Bulk endpoints**: issue N licences (one multi-row insert), revoke a selection, edit expiry and/or notes of a selection, export a selection or the current filter. Up to 1,000 IDs, one transaction, all-or-nothing with an error naming the IDs that blocked it, one audit entry per item.
- Resources: products, customers, licences (+ reveal, message, reset, revoke), activations (+ revoke), audit log, overview stats, backup status, updates (status, check now, install), session (login, logout, logout others), account (password, TOTP, recovery codes), preferences.

---

## 10. Administrator security

- No registration. Admins are created with `clavren-admin create-admin`.
- **Password + TOTP** at every login: Argon2id password; TOTP per RFC 6238 (SHA-1, 6 digits, 30 s, ±1 step); a code can't be reused (`totp_last_used_step`). `create-admin` shows the TOTP QR code in the terminal and 10 one-time recovery codes, and needs a valid code before saving the account. The login failure message never says which factor was wrong.
- **Sessions**: 256-bit random token in an `HttpOnly`, `Secure`, `SameSite=Strict` cookie; only its BLAKE3 hash is stored; expires after 12 hours idle or 7 days; revocable. CSRF token required on every state-changing request.
- **Login rate limit**: 5 failures per 15 minutes per IP and per username.
- **Account changes** (password, recovery codes, authenticator app) ask for the password again; new recovery codes also need the second factor, and a new authenticator needs a code from it. Wrong passwords there count towards the login rate limit, so a stolen session can't be used to guess the password. Changing the password signs out every other session.
- **CLI** (`clavren-admin`, run on the Pi as the `clavren` user): `create-admin`, `reset-password <user>` (also re-enrols TOTP and revokes sessions), `disable-admin` / `enable-admin`, `list-admins`, `show-public-key` (the token-signing key in use and the next one). Changing the signing key is the `CLAVREN_SIGNING_KEY_VERSION` setting, switched once every application in use trusts the next key.

---

## 11. Keys and secrets on the server

- **Master key**: 256 random bits, created on first start in the data directory (`/var/lib/clavren/master.key`, mode `0600`, directory `0700`, owned by `clavren`). Never in the repository, a release package or a log.
- If `master.key` is missing but the database has licences or admin accounts (their keys and TOTP secrets depend on it), Clavren **refuses to start** with a clear message. It never creates a replacement in that situation.
- **Subkeys** derived with `blake3::derive_key`, one fixed versioned context each: licence encryption, licence lookup, token signing (`v1`, `v2`… for rotation), TOTP secret encryption.
- **Hashes are BLAKE3**, except Argon2id for passwords, HMAC-SHA-1 inside TOTP and SHA-512 inside Ed25519 (each required by its standard).
- Use established crates (`blake3`, `chacha20poly1305`, `ed25519-dalek`, `argon2`, `rand`/OS RNG); no hand-written cryptography.

---

## 12. Server behaviour

- **Logging**: `tracing`, one target per module (`clavren::licences`…), level from `CLAVREN_LOG` (default `info`, with PostgreSQL's routine notices from `warn` up), to stdout and the systemd journal, with a request ID per HTTP request. `info` for every successful operation and background task start/finish; `warn` for expected refusals (invalid key, already activated, failed login, rate limit); `error` for unexpected failures with the error chain. Never logged: keys, passwords, tokens, the master key, private keys, customer names or emails. Request bodies are never logged.
- **No silent failures**: every error is returned, logged, or both.
- **Graceful shutdown** on SIGTERM/SIGINT: stop accepting connections, finish in-flight requests (with a timeout), stop background tasks, close the database pool.
- **Concurrency**: independent startup steps (loading the master key, connecting to the database) run in parallel; a batch inserts in one statement; list expansion is one query per field.
- **Client IP**: forwarded headers are trusted only from configured proxies; behind Cloudflare Tunnel the header is `CF-Connecting-IP` (configurable). Otherwise every request would look like it came from `cloudflared`, and one customer could rate-limit everyone.
- **Health**: `/health/live` (process up) and `/health/ready` (database reachable), revealing nothing else.
- **Configuration** by environment variables (`/etc/clavren/clavren.env`): database URL, listen address (localhost by default), data directory, public service URL, token lifetime, check interval hint, rate limits, session lifetimes, trusted proxies and header, backup directory and retention.

---

## 13. Deployment (Raspberry Pi 5, no Docker)

- Raspberry Pi OS 64-bit. **PostgreSQL 17 on the same Pi**, from `apt`, listening on localhost only, with a dedicated `clavren` role and database.
- Clavren is one `aarch64` binary with the dashboard embedded, plus `clavren-admin`. Migrations run at startup.
- `clavren` system user without a login shell; `clavren.service` (systemd) runs as it with `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`, `PrivateTmp`, `ReadWritePaths=/var/lib/clavren`, restart on failure.
- `cloudflared` exposes Clavren over HTTPS; Clavren itself listens on localhost. Nothing in Clavren depends on Cloudflare.
- **Upgrade**: replace the binary, restart; keep the previous binary to roll back, and a fresh backup before every upgrade. From the dashboard: the server only writes request files in `/var/lib/clavren/updates`; `clavren-update-check` (clavren user, timer every 6 hours and on request) finds new releases on GitHub (`CLAVREN_UPDATE_SOURCE`, `off` to stop), and the root `clavren-update` (on request, through a path unit) re-checks the request and the forge, verifies the bundle's BLAKE3 checksum and runs its `install.sh`. Requests are audited as `server.update_requested`.

### Backups

The database and `master.key` are useless without each other, and losing either breaks every customer's licence.

- `clavren-backup.timer` runs nightly (`Persistent=true`, so a missed run happens at next boot).
- It writes one archive with a `pg_dump` (custom format) and `master.key`, plus its BLAKE3 checksum, to a directory **off the SD card** (USB drive or network share). It refuses to run if that directory isn't mounted, so it never fills the SD card.
- Archives are mode `0600`; optionally encrypted with `age` to a configured public key.
- Keeps the last 14; records each success in `backup_runs` (shown on the Overview); a failure exits non-zero, visible in `systemctl --failed` and the journal.
- `docs/backup-and-recovery.md`: a tested restore onto a fresh Pi (install, restore dump and key, verify checksum, start, confirm an existing activation still checks in).

---

## 14. Project structure

One repository: a Rust workspace plus the dashboard. Folders are named after the domain; each feature folder keeps its routes, logic and queries together. Dependencies point one way (HTTP → service → repository).

```text
clavren/
├── crates/
│   ├── clavren-protocol/   # shared by server and client: key format and normalization,
│   │                       # wire types, error codes, check message, token claims, test vectors
│   ├── clavren-server/     # library + binaries `clavren` and `clavren-admin`
│   │   └── src/            # products/ customers/ licences/ activations/ admin_auth/
│   │                       # admin_preferences/ audit/ backups/ crypto/ http/ maintenance/
│   │                       # config.rs logging.rs bin/
│   └── clavren-client/     # activation, checks, offline validation, outcomes,
│                           # store.rs (protected state file) + dpapi.rs (Windows)
├── migrations/
├── web/admin/              # public/ (served as-is), src/ by page: licences/ customers/
│                           # products/ activations/ audit/ account/ preferences/ shared/
├── deploy/                 # clavren.service, clavren-backup.{service,timer,sh}, clavren.env.example
├── docs/                   # spec, design-decisions, development, api, client-integration,
│                           # deployment, backup-and-recovery
└── tests/                  # cross-crate integration and end-to-end
```

No file over about 200 lines and no function over about 20 lines without a strong reason. Every `.rs` and `.md` file has the author header (`metadata` tool).

---

## 15. Testing

Every step ships with its tests; none are left for the end.

- **Unit**: key format and normalization, token claims, error mapping, client outcome mapping, CSV escaping, message template filling.
- **Property-based** (`proptest`): encode/normalize round trips; any casing, spacing or dashes of a valid key normalize to it; any character outside the alphabet is rejected; encrypt/decrypt round trips.
- **Integration** (real PostgreSQL, real migrations, a fresh database per test): every endpoint, success and every error path.
- **Concurrency**: simultaneous activations of one key → exactly one succeeds, no orphan rows; reset racing a check or a revoke ends consistent.
- **Lifecycle**: issue → activate → check → reset → activate elsewhere (old device refused) → revoke (checks refused, key gone, cannot be reset or activated); expiry edit picked up at the next check; token `exp` capped at licence expiry; product deactivation; delete in each state; product deletion; same-device retry is idempotent; deleting a customer keeps their licences working and leaves no name or email anywhere (including audit).
- **Security**: tampered ciphertext and wrong key rejected; replayed, expired or foreign challenges rejected; wrong signature rejected; tokens with bad signature, wrong product, tampered claims or expired rejected; missing master key with existing licences refuses to start; logs never contain keys, passwords, tokens or customer data.
- **Admin**: every admin endpoint refuses anonymous requests and missing CSRF; login with wrong password, wrong or reused TOTP, recovery code used twice, all give the same answer; rate limits; bulk endpoints are all-or-nothing; `expand` without per-row queries; preferences saved, restored, invalid ones rejected.
- **Contract**: pinned JSON for every public response and error, the token claim set, the CSV columns, and golden test vectors (normalization, check message, fingerprint).
- **Migrations**: up from empty, down and up again; data inserted at each step survives the next.
- **Schema**: every table matches its documented functional dependencies; `license_states` matches section 7.
- **Client**: against a real server: offline start, background check, definitive vs temporary failures, clock rollback, a wrong clock, and a clock set back before a check, backoff, nothing on the UI thread.
- **Dashboard E2E** (Playwright, headless, never the real mouse or keyboard): login with TOTP, issue, copy message, reveal, export, inline edit (Enter, blur, Escape), reset, revoke, bulk actions, preferences restored after logging in again; results checked through the API.
- **Rendered**: a reviewed baseline screenshot of every page in light and dark, at normal and phone width; disabled actions dimmed; nothing clipped.
- **Entry points and lifecycle**: server start with and without a master key, graceful shutdown, every CLI command and its output.
- **Backups**: an archive restores into an empty database and licences, activations and reveal still work.
- **Regression**: every bug fix starts with a test that fails without it.

Coverage is gated with `cargo llvm-cov` (minimum raised as it grows, never lowered); the command is in `docs/development.md`.

---

## 16. Build order

Each step is finished (code, tests, docs) before the next:

1. **Foundation**: configuration, logging, database pool, migrations for every table, master key and subkey derivation, health endpoints, graceful shutdown.
2. **Admin access**: `clavren-admin create-admin` with TOTP, login, sessions, CSRF, rate limit, the other CLI commands.
3. **Products and customers**: admin API.
4. **Licences**: issuing (single and bulk), encryption, reveal, message, export, edit, search, filters, audit.
5. **Activation**: atomic, idempotent, error codes, tokens.
6. **Checks**: challenges, signed checks, renewal, reset and revoke.
7. **Client crate**: device identity (DPAPI), activation, offline validation, background checks, outcomes. Applications (avalio-lite first) integrate it in their own repositories, following `docs/client-integration.md`.
8. **Dashboard**: every page, with E2E and rendered tests.
9. **Deployment and backups**: systemd units, backup script, restore test, Raspberry Pi guide.
10. **Docs**: `api.md` with test vectors, `client-integration.md` with a desktop example, `deployment.md`, `backup-and-recovery.md`.

Done when a licence can be issued in the dashboard to a customer, sent with **Copy message**, activated by the client crate against the real server (its end-to-end test), checked, reset and activated on a second computer (the first one locks), and revoked (the second one locks at its next check; for this test, with the client's check interval shortened).
