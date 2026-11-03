[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/02 14:18:54.981076 )
[//]: # (+ Revised: 	2026/11/03 12:36:00.817388 )
[//]: # ( ---------------------------------------------------------------------- )

# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
