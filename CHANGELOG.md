[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/02 14:18:54.981076 )
[//]: # (+ Revised: 	2026/11/03 15:42:16.289467 )
[//]: # ( ---------------------------------------------------------------------- )

# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
