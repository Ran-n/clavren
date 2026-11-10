[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/02 14:18:54.923033 )
[//]: # (+ Revised: 	2026/11/10 08:29:05.578531 )
[//]: # ( ---------------------------------------------------------------------- )

<h1 align="center">Clavren</h1>

<p align="center"><b>Licence keys for Breren apps, checked by Breren's own server.</b><br>
Clavren makes the keys you hand out, makes sure each one works on a single computer, and lets you move or cancel a key whenever you need to.</p>

<p align="center">
  <img src="https://img.shields.io/badge/server-Raspberry_Pi-555" alt="Server: Raspberry Pi">
  <img src="https://img.shields.io/badge/apps-Windows-555" alt="Apps: Windows">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-PBL_v2.0-555" alt="License: PayBack License v2.0"></a>
</p>

> [!NOTE]
> Clavren 0.4.1: the server, the dashboard, the client library and the Raspberry Pi deployment are built and tested, and `deploy/install.sh` installs or upgrades it on a Pi from the release bundle. Apps add it with the client library ([docs/client-integration.md](docs/client-integration.md)); avalio-lite is the first planned.

<p align="center">
  <a href="#why-clavren">Why</a> ·
  <a href="#how-a-key-works">How a key works</a> ·
  <a href="#questions">Questions</a> ·
  <a href="#install-on-a-raspberry-pi">Install</a> ·
  <a href="#run-from-source">Run from source</a> ·
  <a href="#technical-details">Technical details</a> ·
  <a href="#license">License</a>
</p>

---

## Why Clavren?

- **One key, one computer.** A key works on the first computer it's entered on, and nowhere else.
- **Works offline.** Apps keep working for up to 30 days without internet, so a blocked school network or a server outage doesn't lock anyone out.
- **Customers can move.** After a reinstall or a new PC, you free their key in one click and they enter the same key again.
- **You stay in control.** Cancel any key, used or not, and it stops on any computer that goes online within about a day. See who has each key and every computer it has been on.
- **Permanent or until a date.** Sell for good, or for a school year and extend it later.
- **Yours, not a third party's.** It runs on your own Raspberry Pi; no licensing company in the middle.

---

## How a key works

1. You issue a key to a customer in the dashboard, for example `BRN-7KPX-4M9D-QW2R-8NFC`, and **Copy message** gives you a ready-to-send email with it.
2. The customer types the key into the app. Clavren checks it belongs to that app and isn't in use elsewhere, then ties it to that computer.
3. The app starts straight away with its saved licence, and quietly confirms with Clavren once a day whenever it's online. Without internet it keeps working for up to 30 days.
4. If the customer gets a new computer, you **reset** the key; the old computer locks and the same key works on the new one.
5. If you **revoke** a key, it never works again. A computer using it locks within about a day if it's online, or within 30 days if it never goes online.

---

## Questions

<details>
<summary><b>Is it free?</b></summary>

Personal and non-commercial use is free. Commercial use needs a revenue-share agreement under the [PayBack License (PBL) v2.0](LICENSE).
</details>

<details>
<summary><b>What happens if the Raspberry Pi is off?</b></summary>

Apps keep working on their saved licence for up to 30 days. Only a new activation needs the server to be up.
</details>

<details>
<summary><b>Can someone share their key with a friend?</b></summary>

No. Once a key is in use on one computer, entering it on another one is refused.
</details>

<details>
<summary><b>Where do I report a problem?</b></summary>

Open an issue, as described in [Reporting Bugs](CONTRIBUTING.md#reporting-bugs). Never include a licence key or a password in it.
</details>

---

## Install on a Raspberry Pi

Download `clavren-X.Y.Z-aarch64-linux.tar.gz` from the [latest release](https://github.com/Ran-n/clavren/releases/latest), unpack it on the Pi and run:

```sh
sudo ./deploy/install.sh --public-url https://licences.example.com
```

It sets up PostgreSQL, the service, nightly backups, update checks and your admin account. Then point HTTPS (Cloudflare Tunnel or a reverse proxy) at it. When a newer release is out, the dashboard says so, and its **Update** button installs it (backing up first); running the script again from a newer release upgrades too.

[docs/deployment.md](docs/deployment.md) walks through it step by step, with or without the script, with a check after each step.

---

## Run from source

You need [Rust](https://rustup.rs/), [Node.js](https://nodejs.org/) 22 or newer and [PostgreSQL](https://www.postgresql.org/) 17.

```sh
cargo dev    # the server and dashboard, on http://localhost:8080
```

`cargo dev` starts a local, disposable database (never your own), builds the dashboard, asks for a username and sets up the first admin, then runs the server until Ctrl+C. It is for trying and working on Clavren, not for running it for customers: for that, [install it on a Pi](#install-on-a-raspberry-pi).

The full guide is [docs/development.md](docs/development.md).

---

## Technical details

| Document | What's in it |
|----------|--------------|
| [docs/spec.md](docs/spec.md) | The full specification: data model, protocol, security, deployment, tests |
| [docs/design-decisions.md](docs/design-decisions.md) | Why it's built this way and where it departs from the usual defaults |
| [docs/api.md](docs/api.md) | The HTTP API, with test vectors, and the `clavren-admin` commands |
| [docs/client-integration.md](docs/client-integration.md) | Adding licensing to a desktop app with `clavren-client` |
| [docs/deployment.md](docs/deployment.md) | Installing and upgrading on a Raspberry Pi, step by step |
| [docs/backup-and-recovery.md](docs/backup-and-recovery.md) | Nightly backups and restoring onto a fresh Pi |
| [docs/development.md](docs/development.md) | Build, checks, tests, project layout, conventions |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to report bugs, suggest features and submit changes |
| [CHANGELOG.md](CHANGELOG.md) | What changed in each version |

**Built with** Rust (Axum, SQLx, Tokio), PostgreSQL, React and TypeScript.

---

## License

[PayBack License (PBL) v2.0](LICENSE): free for personal and non-commercial use; commercial use requires a revenue-share agreement.
