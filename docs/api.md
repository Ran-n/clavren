[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/02 19:43:36.881392 )
[//]: # (+ Revised: 	2026/11/07 19:37:00.703125 )
[//]: # ( ---------------------------------------------------------------------- )

# HTTP API

Every endpoint Clavren serves. The [specification](spec.md) says why each exists; integrating an
application is in [client-integration.md](client-integration.md).

All bodies are JSON. Times are RFC 3339 in UTC.

---

## Errors

Every error has the same shape and an HTTP status that matches it:

```json
{ "error": "invalid_credentials", "message": "the username, password or code is wrong" }
```

Clients decide what to do from `error`. The `message` is English text for logs and is not meant to
be shown to end users.

| `error` | Status | When |
|---------|--------|------|
| `invalid_request` | 400 | The body isn't valid JSON, a field is missing, unknown or too long, or a value is malformed |
| `invalid_credentials` | 401 | Wrong username, password or code. The same answer covers an unknown or disabled admin |
| `unauthorized` | 401 | No session, or the session expired, was revoked, or its admin was disabled |
| `csrf_failed` | 403 | A state-changing request without this session's `X-CSRF-Token` |
| `not_found` | 404 | Nothing has this ID (a malformed ID too) |
| *conflict codes* | 409 | The request clashes with existing data; each endpoint lists its codes |
| `weak_password` | 422 | A new password shorter than 12 characters (or longer than the maximum) |
| `rate_limited` | 429 | Too many failed attempts. `Retry-After` gives the seconds to wait |
| `internal_error` | 500 | Something failed on the server. The details are only in the server log |

---

## Health

| Endpoint | Answer |
|----------|--------|
| `GET /health/live` | `200 {"status":"ok"}` while the process runs |
| `GET /health/ready` | `200 {"status":"ok"}` when the database answers, `503 {"status":"unavailable"}` when it doesn't |

---

## Application API

What applications call, under `/api/v1`. The [`clavren-client`](../crates/clavren-client) crate
does all of it for Rust applications. Bodies are JSON, up to 16 KiB. Binary values are unpadded
base64url, IDs are lowercase hyphenated UUIDs, and times are RFC 3339 in UTC. **Every answer,
errors included, has `server_time`**, which clients use to notice a clock that was set back.

### `POST /api/v1/activations`

```json
{
  "product": "avalio-lite",
  "key": "brn 79yn 44xq w025 377z",
  "device_public_key": "<32-byte Ed25519 public key>",
  "device_fingerprint": "<optional 32-byte BLAKE3 hash of hardware identifiers>",
  "device_name": "ANA-LAPTOP",
  "platform": "windows-x86_64",
  "app_version": "1.4.0"
}
```

`device_name`, `platform` and `app_version` are 1 to 200 characters. `201`:

```json
{
  "activation": "0190…",
  "token": "eyJhbGciOiJFZERTQSIs…",
  "token_expires_at": "2026-12-02T12:00:00Z",
  "licence_expires_at": null,
  "server_time": "2026-11-02T12:00:00Z"
}
```

The same computer (the same device public key) asking again, because the first answer was lost,
gets `200` with its existing activation and a fresh token.

### `POST /api/v1/activations/{activation}/challenges`

No body. `200`:

```json
{ "challenge": "0190…", "nonce": "<32 random bytes>", "expires_at": "…", "server_time": "…" }
```

A challenge works once, for 2 minutes, and only for its activation. The server keeps only a
BLAKE3 hash of the nonce.

### `POST /api/v1/activations/{activation}/checks`

```json
{ "challenge": "0190…", "nonce": "<the challenge's nonce>", "signature": "<64 bytes>", "app_version": "1.4.1" }
```

`signature` is Ed25519, made with the device's private key, over the UTF-8 bytes of four lines
joined with `\n` and no trailing newline:

```text
clavren-check-v1
<activation id>
<challenge id>
<nonce, base64url>
```

`200` with `token`, `token_expires_at`, `licence_expires_at` and `server_time`. The check records
`last_checked_at` and `app_version`.

### Tokens

A JWT signed with Ed25519: header `{"alg":"EdDSA","typ":"JWT","kid":"v1"}`. Claims:

| Claim | Meaning |
|-------|---------|
| `iss` | The service's public URL |
| `aud` | The product code |
| `sub` | The licence ID |
| `act` | The activation ID |
| `dev` | base64url of the BLAKE3 hash of the device public key |
| `lic_exp` | The licence's expiry (Unix seconds); absent for a permanent licence |
| `iat`, `exp` | Issued and expiry (Unix seconds). `exp` is 30 days after issuing (`CLAVREN_TOKEN_DAYS`), or the licence's expiry if sooner |
| `jti` | A unique ID |

Applications embed the public keys they trust, by `kid`. `clavren-admin show-public-key` prints
them (see below).

### Errors

| `error` | Status | Meaning | Definitive? |
|---------|--------|---------|-------------|
| `invalid_key` | 404 | Malformed, unknown, or for another product (one answer for all) | Ask for another key |
| `already_activated` | 409 | In use on another computer | Ask the seller for a reset |
| `licence_revoked` | 403 | Revoked | Yes |
| `licence_expired` | 403 | Past its expiry date | Yes |
| `product_unavailable` | 403 | The product was deactivated | Yes |
| `activation_revoked` | 403 | This computer was reset or revoked, or the activation doesn't exist | Yes |
| `challenge_invalid` | 401 | Unknown, used, expired or foreign challenge, wrong nonce or signature | Ask for a new challenge |
| `rate_limited` | 429 | See `Retry-After` | No |
| `invalid_request` | 400 | Malformed request | No |
| `internal_error` | 500 | Server failure | No |

**Limits:** 10 activations per hour per client address; 120 challenges and checks per hour per
client address; 30 checks per hour per activation.

### Test vectors

A client in another language can check its implementation against these.

**Key normalization.** The 10 random bytes `3A 7D 52 13 B7 E0 04 51 9C FF` make the key
`BRN-79YN-44XQ-W025-377Z`, canonical form `BRN79YN44XQW025377Z`. All of these normalize to it:
`brn 79yn 44xq w025 377z`, `BRN79YN44XQWO25377Z` (`O` read as `0`), ` brn-79yn44xq-wO25-377z `.
`BRN-79YN-44XQ-W025-377U` (`U` isn't in the alphabet) and `BRN-79YN-44XQ-W025-377` (15
characters) are refused.

**Check message.** Activation `01900000-0000-7000-8000-000000000001`, challenge
`01900000-0000-7000-8000-000000000002`, and the nonce of bytes `00 01 02 … 1F` give:

```text
clavren-check-v1
01900000-0000-7000-8000-000000000001
01900000-0000-7000-8000-000000000002
AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8
```

**Device hash.** The `dev` claim for a device public key of 32 zero bytes is
`KtqDwYGaU3La4SOPwd7RI8gQT9qhWGKq7mlCihgg_No`.

---

## Admin authentication

Everything under `/api/admin` answers with `Cache-Control: no-store`.

**Session.** Signing in sets the `clavren_session` cookie (`HttpOnly`, `SameSite=Strict`, `Secure`
over HTTPS, `Path=/`). A session ends after 12 hours without use or 7 days after sign-in, whichever
comes first. It also ends when the admin signs out, changes their password, or is disabled or
reset with `clavren-admin`.

**CSRF.** Every request except `GET` must send the session's CSRF token in `X-CSRF-Token`. The
token comes back from sign-in and from `GET /api/admin/session`, so a reloaded page can fetch it
again.

**Rate limit.** 5 failed attempts in 15 minutes per client address and per username, after which
sign-in answers `429`. Wrong passwords on the account endpoints count towards the same per-username
limit. A successful sign-in clears that username's count.

**Client address.** Behind Cloudflare Tunnel, the client's address comes from `CF-Connecting-IP`.
That header is believed only from the trusted proxies (localhost by default; see
[development.md](development.md#configuration)).

### `POST /api/admin/session`: sign in

```json
{ "username": "alice", "password": "…", "code": "123456" }
```

`code` is the current code from the authenticator app, or one of the recovery codes. Recovery
codes can be typed in any case, with or without dashes or spaces, and with `I`/`L` for `1` and
`O` for `0`. Each authenticator code and each recovery code works once. The username ignores case.

`200`, with the session cookie:

```json
{
  "admin": { "id": "0190…", "username": "alice" },
  "csrf_token": "…",
  "expires_at": "2026-11-09T12:00:00Z"
}
```

Errors: `invalid_credentials`, `rate_limited`, `invalid_request`.

### `GET /api/admin/session`: the current session

`200` with the same body as sign-in, or `401 unauthorized`.

### `DELETE /api/admin/session`: sign out

`204`. The response clears the cookie.

### `POST /api/admin/session/revoke-others`: sign out everywhere else

`200 {"revoked": 2}`. The number is how many other sessions were ended; this one stays signed in.

### `POST /api/admin/account/password`: change password

```json
{ "current_password": "…", "new_password": "…" }
```

`204`. Every other session of this admin is signed out. Errors: `invalid_credentials` (wrong
current password), `weak_password`, `rate_limited`.

### `POST /api/admin/account/recovery-codes`: new recovery codes

```json
{ "password": "…", "code": "123456" }
```

`200 {"recovery_codes": ["XXXXX-XXXXX-XXXXX-XXXXX-XXXXX-X", …]}` returns 10 new codes and makes
every old code stop working. The new codes are shown only this once.

### `POST /api/admin/account/totp/secret`: start moving to a new authenticator app

`200 {"secret": "BASE32…", "uri": "otpauth://totp/Clavren:alice?…", "qr_svg": "<?xml …<svg …"}`: the
URI also as an SVG QR code, for the dashboard to show. Nothing is stored yet: the old app keeps
working until the new secret is confirmed.

### `PUT /api/admin/account/totp`: confirm the new authenticator app

```json
{ "password": "…", "secret": "BASE32…", "code": "123456" }
```

`code` comes from the new app. `204`; from then on, only the new app's codes work. Errors:
`invalid_credentials` (wrong password), `invalid_request` (the secret is malformed or the code
doesn't match it).

---

## Admin API conventions

Everything below needs a signed-in session, and its CSRF token on every request that isn't a `GET`.

**Lists** are paged: `?limit=50` (1 to 100, default 50) and `?after=<next_cursor>` from the
previous page. The answer is

```json
{ "data": [ … ], "next_cursor": "eyJr…" }
```

`next_cursor` is `null` on the last page. A cursor is opaque: pass it back unchanged, and only to
the same list with the same order and filters.

**Edits** use `PATCH` with only the fields to change. A field set to `null` is cleared; a blank
text field counts as `null`. An edit that changes nothing returns the record without touching it
or the audit log. Unknown fields are refused with `invalid_request`, so a typo never passes
unnoticed.

**Expand.** Related records are IDs unless `?expand=<field>` asks for the whole record. The
parameter repeats or takes a comma-separated list; a dotted field (`licence.customer`) expands a
field of an expanded record. Each list says which fields it expands; any other is
`invalid_request`. A page is expanded with one query per field, never one per row.

**Text** is trimmed. An error names the field it is about.

---

## Products

```json
{
  "id": "0190…",
  "code": "avalio-lite",
  "name": "Avalio Lite",
  "description": null,
  "message_template": "Hello {customer_name},\n\n…{key}…",
  "is_active": true,
  "created_at": "2026-11-02T12:00:00Z",
  "updated_at": "2026-11-02T12:00:00Z"
}
```

- `code` is up to 63 lowercase letters, digits and `-`, starting with a letter or digit. Uppercase
  input is lowercased. Tokens carry it, so it **can't change once the product has licences**.
  On create, without `code` (or with a blank one) the code is made from the name: accents
  dropped, lowercase, other characters as `-`, `product` if nothing is left, and `-2`, `-3`, …
  when taken. A typed code that is taken is refused. Renaming a product never changes its code.
- `name` is up to 200 characters, `description` up to 2,000.
- `message_template` is up to 10,000 characters. Its placeholders are `{customer_name}`,
  `{product_name}` and `{key}`; any other `{word}` is refused. Without a template, a product gets a
  default one.
- Deactivating a product (`"is_active": false`) makes every computer that uses it lock at its
  next check, and keeps everything. Deleting it deletes its licences too, with their keys and
  activations; their computers lock at their next check and their keys read as unknown. Customers
  stay.

| Endpoint | Answer |
|----------|--------|
| `GET /api/admin/products?active=true\|false` | A page of products, A to Z by name |
| `POST /api/admin/products` | `201` with the product. Body: `name`, optional `code`, `description`, `message_template`, `is_active` |
| `GET /api/admin/products/{id}` | The product |
| `PATCH /api/admin/products/{id}` | The product after the change. Any of `code`, `name`, `description`, `message_template`, `is_active` |
| `GET /api/admin/products/{id}/usage` | `{"licences": 120, "in_use": 34}`: what deleting it would take with it |
| `DELETE /api/admin/products/{id}` | `204`. The product, every licence of it (no 1,000 limit), their keys and activations; one `licence.deleted` audit entry per licence and a `product.deleted` with the counts |

Conflict codes: `product_code_taken` (another product has the code), `product_code_locked` (the
product has licences).

---

## Customers

```json
{
  "id": "0190…",
  "name": "Ana López",
  "email": "ana@example.com",
  "notes": null,
  "created_at": "2026-11-02T12:00:00Z",
  "updated_at": "2026-11-02T12:00:00Z"
}
```

- `name` is required, up to 200 characters. `email` is optional and unique ignoring case. `notes`
  are up to 2,000 characters.
- **Deleting a customer erases them.** Their licences keep working, without the link. The audit
  log refers to customers by ID only, so nothing personal remains.

| Endpoint | Answer |
|----------|--------|
| `GET /api/admin/customers?q=…&order=name\|newest` | A page of customers whose name or email contains `q` (ignoring case), A to Z (`name`, the default) or newest first |
| `POST /api/admin/customers` | `201` with the customer. Body: `name`, optional `email`, `notes` |
| `GET /api/admin/customers/{id}` | The customer |
| `PATCH /api/admin/customers/{id}` | The customer after the change. Any of `name`, `email`, `notes` |
| `DELETE /api/admin/customers/{id}` | `204` |

Conflict code: `customer_email_taken`.

---

## Licences

```json
{
  "id": "0190…",
  "product": "0190…",
  "customer": "0190…",
  "state": "redeemed",
  "active_activation": "0190…",
  "created_at": "2026-11-02T12:00:00Z",
  "expires_at": null,
  "revoked_at": null,
  "revocation_reason": null,
  "internal_notes": null
}
```

- `state` is `unused`, `redeemed` (in use on a computer) or `revoked`. It is derived from the
  licence and its activations, never stored.
- `customer` and `active_activation` are `null` when there is none. `expires_at` `null` means
  permanent.
- `?expand=product`, `?expand=customer` and `?expand=active_activation` replace the IDs with the
  records. The parameter repeats, or takes a comma-separated list. An activation shows `id`,
  `licence`, `device_name`, `platform`, `app_version`, `activated_at`, `last_checked_at`,
  `revoked_at` and `revocation_reason` (`reset` or `licence_revoked`).
- Lists and single licences never contain the key. It appears only when issuing, revealing,
  copying the message and exporting, and the last three are recorded in the audit log.

### `POST /api/admin/licences`: issue

```json
{
  "product": "0190…",
  "customer": { "name": "Ana López", "email": "ana@example.com" },
  "quantity": 2,
  "expires_at": "2027-07-01T00:00:00Z",
  "internal_notes": "School order 42"
}
```

`customer` is an existing customer's ID, a new customer (created in the same transaction), or
absent. `quantity` is 1 to 1,000 (default 1). `expires_at` must be in the future. `201`:

```json
{
  "licences": [ { "id": "…", "product": "…", "state": "unused", …, "key": "BRN-7KPX-4M9D-QW2R-8NFC" } ],
  "customer": { "id": "…", "name": "Ana López", … },
  "message": "Hello Ana López, …"
}
```

`licences` come in the order issued. `message` is the product's template filled in, with every
key on its own line where `{key}` is; it is `null` without a customer. Errors: `invalid_request`
(including an unknown product or customer), `product_inactive`, `customer_email_taken`.

### Finding licences

`GET /api/admin/licences` takes:

| Parameter | Meaning |
|-----------|---------|
| `q` | A key typed in any form (found by its hash), a licence ID, or part of the customer's name or email |
| `product`, `customer` | IDs |
| `state` | `unused`, `redeemed` or `revoked` |
| `expiry` | `permanent`, `expiring` (within 30 days) or `expired` |
| `not_product`, `not_customer`, `not_state`, `not_expiry`, `not_q` | The same values, hiding what they match; licences without a customer are never hidden by a customer exclusion |
| `order` | `newest` (default) or `expiry` (soonest first, permanent last) |
| `limit`, `after`, `expand` | As everywhere |

The filters combine: `?q=acme&not_q=test@acme.com&not_state=revoked` lists Acme's live licences
except that address's. A contradiction (`state=revoked&not_state=revoked`) is an empty list.

`GET /api/admin/licences/{id}` returns one licence. `expand` works there too, and on the answers
of `PATCH`, `revoke` and `reset` below.

### Changing a licence

| Endpoint | Body | Answer |
|----------|------|--------|
| `PATCH /api/admin/licences/{id}` | Any of `customer` (an existing customer's ID, `{"name", "email"}` for a new one, or `null` for none), `expires_at` (`null`: permanent; a past date ends it now), `internal_notes`, `revocation_reason` (revoked licences only). Every other field (product, key, state, dates) is read-only and refused | The licence |
| `POST /api/admin/licences/{id}/revoke` | `{}` or `{"reason": "…"}` | The licence. The key is deleted and can never be accepted again; a computer using it locks at its next check |
| `POST /api/admin/licences/{id}/reset` | | The licence, now `unused`. The same key activates on another computer; the old one locks at its next check |
| `DELETE /api/admin/licences/{id}` | | `204`. The licence, its key and its activations are gone in any state; a computer using it locks at its next check, and the key reads as unknown (`invalid_key`) |
| `POST /api/admin/licences/bulk/edit` | `ids` (up to 1,000) and the fields of `PATCH`; a new customer is created once for the batch | `{"updated": 3}` |
| `POST /api/admin/licences/bulk/revoke` | `ids`, optional `reason` | `{"revoked": 3}` |
| `POST /api/admin/licences/bulk/delete` | `ids`, optional `"dry_run": true` (only counts) | `{"deleted": 12, "in_use": 3}` |

Bulk requests change every licence or none. When some licences block the batch, the error lists
them:

```json
{ "error": "licence_revoked", "message": "some licences are already revoked", "ids": ["0190…"] }
```

Errors: `not_found` (with `ids` for a batch), `licence_revoked`, `licence_not_in_use` (reset),
`licence_not_revoked` (a revocation reason for a live licence, with `ids`), `customer_email_taken`
(a new customer with an existing email), `invalid_request` (including an empty batch, more than
1,000 IDs, an ID listed twice, or an unknown customer). The audit names the changed fields
(`licence.updated` with `fields`) and the state a deleted licence was in (`licence.deleted` with
`was`), never names, emails or reason text.

### The key, again

| Endpoint | Answer |
|----------|--------|
| `POST /api/admin/licences/{id}/reveal` | `{"key": "BRN-…"}` |
| `POST /api/admin/licences/{id}/message` | `{"message": "…"}`, the product's template filled in |
| `POST /api/admin/licences/export` | CSV, for `{"ids": [...]}` (up to 1,000) or `{"filter": {"state": "unused", …}}` (the list filters, exclusions included, up to 10,000 licences) |

A revoked licence has no key left: `licence_revoked`. The CSV has the columns `licence_id,
product_code, customer_name, customer_email, key, state, issued_at, expires_at, notes`, with CRLF
line ends. A cell that a spreadsheet would run as a formula (starting with `=`, `+`, `-`, `@`, tab
or carriage return) starts with `'`.

## Activations

An activation is one computer using one licence. Device public keys never leave the server.

`GET /api/admin/activations` lists them, newest first:

| Parameter | Meaning |
|-----------|---------|
| `licence`, `product` | IDs |
| `state` | `active` or `revoked` |
| `q` | Part of the device name, platform or app version |
| `limit`, `after` | As everywhere |
| `expand` | `licence`, and inside it `licence.product`, `licence.customer`, `licence.active_activation` |

Each row has `id`, `licence`, `device_name`, `platform`, `app_version`, `activated_at`,
`last_checked_at`, `revoked_at` and `revocation_reason` (`reset`, `licence_revoked`, or `null` while active).

`POST /api/admin/activations/{id}/revoke` ends the activation by resetting its licence, so the
same key works on another computer. Answer: the activation. Errors: `not_found`,
`activation_revoked` (409) when it has already ended.

## Overview

`GET /api/admin/overview` returns the numbers for the dashboard's first page:

```json
{
  "active_products": 1, "inactive_products": 0,
  "unused": 12, "redeemed": 40, "revoked": 2,
  "active_computers": 40, "computers_checked_last_24h": 31, "checks_last_24h": 33,
  "last_backup": { "id": "0190…", "finished_at": "2027-01-15T03:00:12Z", "archive_name": "clavren-20270115.tar", "size_bytes": 20480 },
  "backup_overdue": false
}
```

`computers_checked_last_24h` counts computers in use that checked in (or activated) in the last 24
hours; `checks_last_24h` counts successful checks, by the hour (the current hour and the 23 before
it). `backup_overdue` is `true` when there is no backup or the last one is more than 48 hours old.

## Audit log

`GET /api/admin/audit` lists what admins and the application API did, newest first:

| Parameter | Meaning |
|-----------|---------|
| `action` | An action (`licence.revoked`) or a prefix ending in a dot (`licence.`) |
| `admin`, `entity` | IDs |
| `entity_type` | `licence`, `product`, `customer`, `activation`, `admin_user`, … |
| `from`, `to` | RFC 3339 times; `from` included, `to` not |
| `limit`, `after` | As everywhere |
| `expand` | `admin_user` (`{id, username}`) |

Each entry has `id`, `admin_user` (`null` for the application API), `action`, `entity_type`,
`entity_id`, `metadata`, `ip_address` and `created_at`. Entries are never changed or deleted.

## Backups

`GET /api/admin/backups` returns `{"data": [...]}`: the last 14 backup runs, newest first, each
with `id`, `finished_at`, `archive_name` and `size_bytes`. Backups are made by the backup timer
(see [deployment.md](deployment.md)); the server only reports them.

## Preferences

What the dashboard remembers for each admin, on any browser. `GET /api/admin/preferences` returns
them (the defaults until the admin saves any); `PUT /api/admin/preferences` replaces them and
returns what was saved.

```json
{
  "version": 1,
  "theme": "dark",
  "last_page": "/licences",
  "tables": {
    "licences": { "filters": { "state": "unused" }, "search": "López", "order": "expiry", "page_size": 25, "columns": ["key", "customer"] }
  }
}
```

`theme` is `system` (the default), `light` or `dark`. Tables are `licences`, `customers`,
`products`, `activations`, `audit` and `backups`. `page_size` is 10–100; texts are up to 200
characters. Unknown fields, another `version`, and anything that reads as a licence key (in
`search` or a filter) are refused with `invalid_request`, so a key typed into a search box is
never stored.

---

## `clavren-admin`

Admins are managed on the server with `clavren-admin`, which reads the same `CLAVREN_*` variables
as the server. On the Pi, run it as the `clavren` user so it can read the master key.

| Command | What it does |
|---------|--------------|
| `create-admin <username>` | Asks for a password twice, shows a QR code (and the key, to type in by hand) for the authenticator app, and saves the admin only after a valid code. Prints 10 recovery codes |
| `reset-password <username>` | Same steps for an existing admin: new password, new authenticator, new recovery codes. Signs out all their sessions |
| `disable-admin <username>` | Stops the admin signing in and ends their sessions |
| `enable-admin <username>` | Lets a disabled admin sign in again |
| `list-admins` | Username, status, creation date and last sign-in of every admin |
| `show-public-key` | The token-signing public key in use (`v1`) and the next one, to embed in a new app version before switching `CLAVREN_SIGNING_KEY_VERSION` |

A new username has up to 63 characters: lowercase letters, digits, `.`, `_` and `-`, starting with
a letter or digit. The other commands find an admin's username whatever its case. Every change is
recorded in the audit log.
