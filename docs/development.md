[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/02 14:18:55.213599 )
[//]: # (+ Revised: 	2026/11/03 04:54:25.527685 )
[//]: # ( ---------------------------------------------------------------------- )

# Development

How to build, check and work on Clavren. What it must do is in [spec.md](spec.md); why it is
built this way is in [design-decisions.md](design-decisions.md).

---

## Prerequisites

- [Rust](https://rustup.rs/): `rustup` installs the exact toolchain pinned in
  `rust-toolchain.toml` (1.99.0, with clippy, rustfmt and the Raspberry Pi target) on first use.
- [Node.js](https://nodejs.org/) 22 or newer, for the admin dashboard.
- [PostgreSQL](https://www.postgresql.org/) 17, for running the server and the integration tests.

## Layout

| Path | What it holds |
|------|---------------|
| `crates/clavren-protocol/` | Everything server and clients must agree on: wire types, error codes, key format, challenge message, token claims |
| `crates/clavren-server/` | The service: public API, admin API, dashboard serving, background tasks, and the `clavren-admin` CLI |
| `crates/clavren-client/` | The library Breren applications use to activate, check and renew a licence |
| `web/admin/` | The admin dashboard (React, TypeScript, Vite), embedded in the server binary |
| `docs/` | Specification, design decisions and the guides |

Inside each crate, folders are named after the domain (`licences/`, `products/`,
`activations/`…). Dependencies point one way: HTTP → service → repository.

## Build and check

```sh
cargo build
cargo clippy --all-targets -- -D warnings
cargo fmt --check
.\scripts\local-db.ps1   # once per session: the local database (below)
cargo test
.\scripts\coverage.ps1   # the full suite under coverage, with the line gate
```

## Tests

Development and tests use a **local, disposable PostgreSQL cluster** in `target/local-postgres`,
on `127.0.0.1:55432` with passwordless local access. `scripts/local-db.ps1` creates and starts it
(`-Stop` stops it, `-Reset` deletes it with all development data); it never touches another
PostgreSQL server. `.cargo/config.toml` points `DATABASE_URL` at it, and every database test
creates and drops its own database there (`#[sqlx::test]`), so tests never see each other's data
or the development database. An explicit `DATABASE_URL` in the environment wins.

| File | What it covers |
|------|----------------|
| `src/**` (`#[cfg(test)]`) | Unit and property tests next to the code: configuration, master key file, key derivation (with golden values cross-checked with `b3sum`), encryption, passwords, TOTP, recovery codes, cookies, CSRF tokens, client addresses, the rate limiter, error shapes |
| `tests/migrations.rs` | Migrations apply, undo and apply again; re-running keeps data |
| `tests/schema.rs` | The rules the database enforces itself, and the pinned column list of every table |
| `tests/startup.rs` | Startup against a real database: master key created or reused or refused, errors, clean shutdown, no secrets in logs |
| `tests/health.rs` | The health endpoints, request IDs, and what request logs contain |
| `tests/entry_point.rs` | The `clavren` and `clavren-admin` binaries: exit codes and messages, and working for real |
| `tests/admin_cli.rs` | `clavren-admin` commands through a scripted terminal that answers with real authenticator codes |
| `tests/admin_login.rs` | Sign-in: one answer for every failure, one-time codes, recovery codes, rate limits, addresses behind the tunnel, no secrets in logs or the audit log |
| `tests/admin_session.rs` | Session expiry, sign-out, signing out other sessions, CSRF checks, disabled admins |
| `tests/admin_account.rs` | Changing password, recovery codes and authenticator app |
| `tests/maintenance.rs` | The hourly clean-up and its shutdown |
| `tests/products.rs` | Products: create, list, edit, delete; the code lock; audit; session and CSRF |
| `tests/customers.rs` | Customers: create, search, order, edit, delete without leaving personal data |
| `tests/licences.rs` | Issuing (keys only as hash and ciphertext), customers typed in place, the message, search by key, name or ID, filters, orders, `expand` |
| `tests/licence_lifecycle.rs` | Edit, revoke, reset, bulk all-or-nothing, reveal, message, CSV export and its formula guard |
| `tests/activation.rs` | Activating a key: one computer per licence, the same computer again, refusals, rate limits, two computers racing for one key |
| `tests/checks.rs` | Challenges and checks: signed tokens, the device signature, replayed and expired challenges, reset and revoked licences, per-activation limits |
| `tests/admin_console.rs` | Activations list and revoke, the overview and its backup warning, the audit log, backups, preferences |
| `tests/dashboard.rs` | The embedded dashboard: its files, every page path, cache and security headers, API paths that stay JSON errors |
| `tests/backup_restore.rs` | A `pg_dump` archive restores into an empty database and everything still works with the same master key; the deploy scripts parse |
| `crates/clavren-client/tests/offline_and_online.rs` | The client against a fake server: offline days, refusals, clock rollback, the background checker |
| `crates/clavren-client/tests/against_server.rs` | The client against the real server over HTTP: activate, check, reset, a second computer, revoke |

Tests that check logs use `common::capture_logs`. It installs one global subscriber that sends each
line to the capture buffer of the thread that logged it. A scoped subscriber per test
(`set_default`) loses lines now and then when tests run in parallel.

The coverage gate (`scripts/coverage.ps1`) lists the lines left uncovered on purpose and why;
everything else it reports needs a test.

## Running the server locally

```powershell
.\scripts\local-db.ps1   # once per session
cargo run
```

Then `http://localhost:8080/health/ready` answers `{"status":"ok"}`. Stop it with Ctrl+C.

`cargo run` uses the development defaults in `.cargo/config.toml`: the `clavren_dev` database in
the local cluster, `http://localhost:8080`, and `target/dev-data` for the master key. Cargo only
applies them when a variable isn't already set, and an installed binary never sees them.

### Configuration

The server reads its configuration from environment variables:

| Variable | Required | Default | Meaning |
|----------|----------|---------|---------|
| `CLAVREN_DATABASE_URL` | yes | | `postgres://user:password@host:port/database` |
| `CLAVREN_PUBLIC_URL` | yes | | The address apps and the dashboard use, e.g. `https://licences.example.com` (`http://` only for localhost) |
| `CLAVREN_LISTEN` | no | `127.0.0.1:8080` | Address and port to listen on |
| `CLAVREN_DATA_DIR` | no | `/var/lib/clavren` | Where `master.key` lives (created on first start) |
| `CLAVREN_LOG` | no | `info` (PostgreSQL notices from `warn`); `warn` for `clavren-admin` | Log filter, e.g. `debug` or `info,clavren=debug` |
| `CLAVREN_TRUSTED_PROXIES` | no | `127.0.0.1,::1` | Comma-separated proxy addresses whose client-address header is believed (`cloudflared`), or `none` |
| `CLAVREN_CLIENT_IP_HEADER` | no | `cf-connecting-ip` | The header that holds the client's address |
| `CLAVREN_SESSION_IDLE_HOURS` | no | `12` | An admin session ends after this long unused |
| `CLAVREN_SESSION_MAX_DAYS` | no | `7` | An admin session ends this long after sign-in, used or not |
| `CLAVREN_LOGIN_MAX_FAILURES` | no | `5` | Failed sign-ins allowed per address and per username within the window |
| `CLAVREN_LOGIN_WINDOW_MINUTES` | no | `15` | The window for counting failed sign-ins |
| `CLAVREN_TOKEN_DAYS` | no | `30` | How long an application works offline after its last successful check (capped at the licence's expiry) |
| `CLAVREN_SIGNING_KEY_VERSION` | no | `1` | Which token-signing key is in use; see `clavren-admin show-public-key` before changing it |

With an `http://` public URL (localhost only), the session cookie is sent without `Secure`, so the
dashboard works on `http://localhost`.

### An admin account

```powershell
cargo run --bin clavren-admin -- create-admin ran
```

It asks for a password, shows a QR code for the authenticator app, and asks for a code from the
app before saving anything. The commands are listed in [api.md](api.md#clavren-admin).

### Dashboard

```sh
cd web/admin
npm ci
npm run build      # type-checks, then builds into web/admin/dist
npm run dev        # dev server; forwards /api and /health to a local Clavren on port 8080
npm run typecheck  # the dashboard and its tests
npm run e2e        # end-to-end and screenshot tests (below)
```

`cargo build` embeds `web/admin/dist` in the server (`crates/clavren-server/build.rs`), so build
the dashboard first. Without it the server still builds, and its pages say how to build the
dashboard.

**End-to-end tests** (`web/admin/e2e`, Playwright, headless Chromium; nothing touches the real
mouse or keyboard). `npm run e2e` builds the dashboard, then starts two servers from
`crates/clavren-server/examples/e2e_server.rs` on ports 8091 and 8092, each with its own fresh
database in the local cluster (`.\scripts\local-db.ps1` must be running) and a clock frozen at
2027-03-01 09:30 UTC. The `flows` project signs in, issues, copies, reveals, exports, edits in
place, resets, revokes, works in bulk, and checks every result through the API. The `rendered`
project compares every page, light and dark, at desktop and phone width, with the reviewed
baselines in `web/admin/e2e/baselines`, and checks that no page scrolls sideways and that disabled
actions are dimmed. After a deliberate visual change, run
`npm run e2e -- --project=rendered --update-snapshots`, look at every changed image, and commit
the ones that are right.

## Conventions

- Every `.rs` and `.md` file starts with the author header, maintained with the `metadata` tool
  (`metadata -c` on a new file, `metadata -m` after each edit).
- No source file over about 200 lines and no function over about 20 lines without a strong
  reason.
- Logging goes through `tracing`, one target per module; secrets are never logged.
- Every change comes with tests of every applicable kind (see the spec, section 15).
