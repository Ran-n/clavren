[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/02 14:18:54.981076 )
[//]: # (+ Revised: 	2026/11/09 16:06:16.174913 )
[//]: # ( ---------------------------------------------------------------------- )

# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0] - 2026-11-09

### Added

- The dashboard shows the server's version and tells you when a newer Clavren release is out: a notice on every page with what's new, and on the Overview the last check and **Check now**. A timer checks GitHub every 6 hours (`CLAVREN_UPDATE_SOURCE`: another https:// address, or `off` to stop).
- Update the server from the dashboard: an **Update** button installs a new release, backing up first (or, with no backup drive mounted, only after you tick "Update without a backup"), checks the download's BLAKE3 checksum, and the dashboard reloads itself on the new version. Requests are audited as `server.update_requested`.
- The Licences page remembers whether the Issue panel is open, and the product, quantity and expiry chosen in it (never the customer or notes), on any browser; after issuing it is remembered closed.
- Formatting, lints, tests and the dashboard's type check run on every push, on GitHub, GitLab and Codeberg (`scripts/ci.sh`).

### Fixed

- Dashboard: after signing in again in another tab, every change (**Check now**, issuing, editing, deleting) failed with "This page is out of date"; it now picks up the new session and goes through, or, when another admin signed in, switches to that account and says so.
- `clavren-restore.sh` left Clavren stopped when `pg_restore` failed; it now starts it again on the unchanged database and says so.
- Client: setting the clock back before an online check made the 30-day token last as long as the clock was set back; tokens are now judged by the server's time, so a wrong clock still works and a clock set back gains nothing. A clock set ahead no longer locks a computer out.
- Client: a `200` answer that isn't Clavren's (a proxy's or captive portal's page) wiped the saved licence; it now changes nothing.
- Client: a token it can't verify (an untrusted signing key, another activation) was stored, and the background checker then asked again in a tight loop; such tokens are refused and the check backs off.
- Client: `status()` wrote the state file on almost every call, on the UI thread; it no longer touches the disk, and the background checker saves the time seen every ten minutes and when it stops (`save_time_seen()` for applications without one).
- Client: a saved licence that couldn't be read at start (the file in use) was overwritten by the next activation; it is now left alone until the application starts again.
- Client: the state file is flushed to disk before it replaces the old one, the buffers that carry the state (DPAPI's output, the serialized state, a new device key's seed) are wiped after use, a check answer for an activation replaced meanwhile is ignored, a panicking `on_change` callback no longer ends background checks, and a check interval too long for the calendar means "never" instead of panicking.
- Sign-in attempts sent in parallel all got past the limit before any was counted, each running Argon2 (19 MiB) at once; attempts are now counted first and at most two password checks run at a time.
- The per-username sign-in limit counted `alice` and `alice ` separately; every spelling of a username now shares one limit.
- Rate limits counted each IPv6 address on its own, so one /64 network had endless attempts; they now count per /64. With `X-Forwarded-For`, the last entry (the one the proxy added) is used instead of the first, which the client chooses.
- Ending a computer's activation could end another computer that took over the licence meanwhile; it now refuses with `activation_revoked`.
- A licence edit that changed nothing was still written and audited; only the licences and fields that change are, as the API documentation says.
- Lengths of time in the configuration (session, sign-in window, token) are limited to ten years; a larger value made sign-ins or checks fail.
- A stored password hash or authenticator secret that can't be read, a failed stand-in password check, and recovery codes that don't read back are now logged or refused instead of passing silently.
- `cargo dev` rebuilt nothing when it couldn't read the dashboard sources, and now says why; it also stops a local database it started when starting it failed half-way, and recognises the local database when its password contains `@`.
- Dashboard: when the saved settings couldn't be loaded, the first change (or just opening a page) saved the defaults over them; nothing is saved until the page is reloaded, and the notice says so.
- Dashboard: a setting changed just before signing out was lost; it is saved first (and a failed save keeps you signed in, with the error). Settings are saved one after another, so an older save can't undo a newer one.
- Dashboard: signing out of a session that had already ended left the dashboard on screen; it goes back to signing in.
- Dashboard: pressing Enter in the customer field of the Issue panel issued the licences at once; it no longer submits the form.
- Dashboard: a list that failed to load also said it was empty ("No licences yet"); only the error shows now.
- Dashboard: unticking a row while "all that match" was selected unticked the other rows; it now leaves out only that row, on every page, and the bar says how many are left out.
- Dashboard: a deleted product left in the Activations product filter hid every computer; it is dropped, as on Licences.
- Dashboard: a wrong password on the account forms said "The username, password or code is wrong"; each form now names what is wrong, and a password over 1,024 characters is caught before sending.
- Dashboard: a computer's history showed at most 100 activations, a slow answer about one product could replace the question about another on the Products page, an unreadable remembered date in the audit filters could blank the page, and Copy on plain HTTP said "Something unexpected happened"; all fixed.
- Dashboard: numbers are written the same way everywhere (`en-GB`, like the dates); inline edit buttons describe their value to screen readers, a disabled button's reason reaches them too, and the table pager is named "Table pages" apart from the main navigation.

### Changed

- `docs/spec.md` describes the client's clock handling as built (the server's time, measured at each answer) and the device check as the token's `dev` claim.
- Activations have an index by licence (migration `20261108000001`), so deleting licences no longer scans every activation ever made.
- Dependencies: patch updates of ten Rust crates, Vite 8.3.6 and Playwright 1.65.0.

## [0.3.0] - 2026-11-08

### Added

- Licences can be filtered out as well as in: Except options for state, product and expiry, a hide box for key, customer or licence ID, and the same `not_*` filters in the API and CSV export.
- Every licence field but the product can be edited after issuing: the customer (added, changed or removed) and the revocation reason join expiry and notes, inline and in bulk.
- Licences can be deleted in any state, one at a time or in bulk; a computer using one locks at its next check.
- A new product's code is optional: left empty, it is made from the name (`Ñandú Pro` → `nandu-pro`, then `-2`, `-3`, … when taken), in the dashboard and the API.

### Changed

- The customer field lists every customer A to Z and narrows by name as you type, instead of showing five suggestions after two letters.
- Deleting a product now deletes its licences too, after typing its code to confirm; its computers lock at their next check.

## [0.2.0] - 2026-11-03

### Added

- `cargo dev` (the `xtask` crate): starts the local database, builds the dashboard when it changed, creates the first admin when there is none, and runs the server; it stops the database it started on the way out.
- `deploy/install.sh`: installs Clavren on a Raspberry Pi (PostgreSQL, a random database password, the user, folders, configuration, services, the first backup and the first admin) or upgrades it (backup first, old binaries kept as `*.previous`). Tested on a Raspberry Pi 3 B+ with Debian 13.
- `clavren-new-admin`, installed with the server: creates an admin on the terminal's alternate screen, so the username, QR code and recovery codes never stay in the scrollback.

### Fixed

- `docs/deployment.md` described cross-compiling instead of the release bundle, and its `create-admin` command broke on configuration values with spaces or quotes. It is now a step-by-step guide, with the script or entirely by hand, a check after every step, HTTPS with Cloudflare Tunnel or Caddy, day-to-day commands, a `clavren-admin` shell shortcut and troubleshooting.
- `docs/deployment.md` also covers what the first real installation needed: a fixed address for the Pi, Cloudflare Tunnel set up entirely over SSH (the approval link opens on any device), changing the public address, and the Pi 3 B+ and Debian 12+ as supported.
- `docs/backup-and-recovery.md`: restoring onto a new Pi works after either kind of installation, and warns not to restore the empty backup the installation makes; a one-off backup to another computer while there is no backup drive.
- `clavren-backup.sh` warned `find: Failed to restore initial working directory` when run by hand from a home folder.
- A bare `cargo run` failed with "could not determine which binary to run"; it now runs the server.

## [0.1.0] - 2026-11-03

### Added

- Server foundation: configuration from `CLAVREN_*` variables, structured logging, PostgreSQL migrations for every table, the master key and its derived subkeys, health endpoints, graceful shutdown.
- Admin access: `clavren-admin` (`create-admin`, `reset-password`, `disable-admin`, `enable-admin`, `list-admins`, `show-public-key`), sign-in with password and TOTP or a recovery code, sessions, CSRF protection, rate limits, account changes, hourly clean-up.
- Products and customers, with Stripe-style `?expand=` (also inside an expanded record, e.g. `licence.customer`) and cursor pagination.
- Licences: issuing up to 1,000 at once with a customer typed in place, keys stored only as a keyed hash and ciphertext, reveal, the filled-in message, CSV export with a formula guard, search by key, name, email or ID, filters, editing, reset, revoke, and all-or-nothing bulk actions.
- The application API: activation (one computer per licence, safe to retry), signed challenges and checks, EdDSA tokens valid offline for 30 days.
- `clavren-client`: activation, DPAPI-protected state, offline validation, clock-rollback detection, a background checker with backoff, and typed outcomes.
- The admin dashboard, embedded in the server: Overview, Licences (issue panel, bulk actions, detail page), Customers, Products, Activations, Audit log and Account, with inline editing, inline confirmations, light and dark themes, and preferences remembered on the server.
- Successful checks counted by the hour (`check_counts`, kept 30 days), shown on the Overview as the checks of the last 24 hours.
- Raspberry Pi deployment: systemd units, a nightly backup of the database and master key to a drive off the SD card, and a restore script.
- Documentation: `api.md` with test vectors, `client-integration.md`, `deployment.md`, `backup-and-recovery.md`.
