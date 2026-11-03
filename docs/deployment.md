[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/03 04:54:17.627908 )
[//]: # (+ Revised: 	2026/11/03 04:54:17.627908 )
[//]: # ( ---------------------------------------------------------------------- )

# Deployment

Clavren on a Raspberry Pi 5 with Raspberry Pi OS (64-bit), PostgreSQL on the same Pi, and
Cloudflare Tunnel in front. No Docker. Backups and restoring are in
[backup-and-recovery.md](backup-and-recovery.md).

---

## What runs where

| Piece | Where |
|-------|-------|
| `clavren` (server, dashboard embedded) | `/usr/local/bin/clavren`, run by `clavren.service` as the `clavren` user |
| `clavren-admin` | `/usr/local/bin/clavren-admin`, run by hand as `clavren` |
| Configuration | `/etc/clavren/clavren.env` (root:clavren, 0640) |
| `master.key` | `/var/lib/clavren/master.key` (clavren, 0600; directory 0700) |
| PostgreSQL 17 | from `apt`, listening on localhost only |
| Backups | `clavren-backup.timer` → `/usr/local/lib/clavren/clavren-backup.sh`, to a USB drive |
| HTTPS | `cloudflared`, forwarding to `127.0.0.1:8080` |

Clavren listens on localhost only. Nothing in it depends on Cloudflare: any reverse proxy works if
`CLAVREN_TRUSTED_PROXIES` and `CLAVREN_CLIENT_IP_HEADER` match it.

---

## Building

On a development machine, build the dashboard first (the binary embeds `web/admin/dist`), then
cross-compile for the Pi:

```sh
cd web/admin && npm ci && npm run build && cd ../..
rustup target add aarch64-unknown-linux-gnu
cargo install cross   # or build on the Pi itself with `cargo build --release`
cross build --release --target aarch64-unknown-linux-gnu --bin clavren --bin clavren-admin
```

The binaries are in `target/aarch64-unknown-linux-gnu/release/`. Building on the Pi takes longer
but needs no cross toolchain.

---

## First installation

1. **Packages**:

   ```sh
   sudo apt update
   sudo apt install postgresql postgresql-client b3sum age
   ```

   PostgreSQL listens on localhost only by default; leave it that way.

2. **Database and role** (choose a long random password):

   ```sh
   sudo -u postgres createuser --pwprompt clavren
   sudo -u postgres createdb --owner clavren clavren
   ```

3. **User and folders**:

   ```sh
   sudo useradd --system --home-dir /var/lib/clavren --shell /usr/sbin/nologin clavren
   sudo install -d -o clavren -g clavren -m 0700 /var/lib/clavren
   sudo install -d -o root -g clavren -m 0750 /etc/clavren /usr/local/lib/clavren
   ```

4. **Files** from the repository and the build:

   ```sh
   sudo install -m 0755 clavren clavren-admin /usr/local/bin/
   sudo install -m 0750 -g clavren deploy/clavren-backup.sh deploy/clavren-restore.sh /usr/local/lib/clavren/
   sudo install -m 0644 deploy/clavren.service deploy/clavren-backup.service deploy/clavren-backup.timer /etc/systemd/system/
   sudo install -m 0640 -g clavren deploy/clavren.env.example /etc/clavren/clavren.env
   sudo nano /etc/clavren/clavren.env   # database password, public URL, backup directory
   ```

5. **Backup drive**: mount a USB drive (or network share) at `/mnt/clavren-backups` through
   `/etc/fstab` (`nofail`, so the Pi still boots without it), and give it to `clavren`:

   ```sh
   sudo install -d -o clavren -g clavren -m 0700 /mnt/clavren-backups
   ```

   For another path, change `CLAVREN_BACKUP_DIR` and the two paths in
   `clavren-backup.service`.

6. **Start**:

   ```sh
   sudo systemctl daemon-reload
   sudo systemctl enable --now clavren clavren-backup.timer
   systemctl status clavren
   curl -s http://127.0.0.1:8080/health/ready
   ```

   The first start creates `master.key` and runs the migrations. **Make a backup now**
   (`sudo systemctl start clavren-backup`) and keep a copy of the archive somewhere else: without
   `master.key` no licence key can be read and no TOTP code checked.

7. **The first admin**:

   ```sh
   sudo -u clavren env $(sudo cat /etc/clavren/clavren.env | grep -v '^#' | xargs) clavren-admin create-admin ran
   ```

   It shows a QR code for the authenticator app and prints 10 recovery codes: store them offline.

8. **Cloudflare Tunnel**: install `cloudflared`, create a tunnel, and route your hostname to
   `http://127.0.0.1:8080`. `CLAVREN_PUBLIC_URL` must be that `https://` hostname. Clavren takes
   the client address from `CF-Connecting-IP` only when the request comes from `cloudflared` on
   localhost.

---

## Configuration

`/etc/clavren/clavren.env`, read by the server, `clavren-admin` and the backup script:

| Variable | Default | Meaning |
|----------|---------|---------|
| `CLAVREN_DATABASE_URL` | (required) | `postgres://clavren:password@127.0.0.1:5432/clavren` |
| `CLAVREN_PUBLIC_URL` | (required) | The `https://` address apps and the dashboard use |
| `CLAVREN_LISTEN` | `127.0.0.1:8080` | Address and port |
| `CLAVREN_DATA_DIR` | `/var/lib/clavren` | Where `master.key` lives |
| `CLAVREN_LOG` | `info` | Log filter, e.g. `debug` or `info,clavren=debug` |
| `CLAVREN_TRUSTED_PROXIES` | `127.0.0.1,::1` | Proxies whose client-address header is believed, or `none` |
| `CLAVREN_CLIENT_IP_HEADER` | `cf-connecting-ip` | That header |
| `CLAVREN_SESSION_IDLE_HOURS` | `12` | Admin session ends after this long unused |
| `CLAVREN_SESSION_MAX_DAYS` | `7` | Admin session ends this long after sign-in |
| `CLAVREN_LOGIN_MAX_FAILURES` | `5` | Failed sign-ins per address and per username in the window |
| `CLAVREN_LOGIN_WINDOW_MINUTES` | `15` | That window |
| `CLAVREN_TOKEN_DAYS` | `30` | Days an application works offline after its last check |
| `CLAVREN_SIGNING_KEY_VERSION` | `1` | Token-signing key in use (see below) |
| `CLAVREN_BACKUP_DIR` | (required for backups) | The backup drive |
| `CLAVREN_BACKUP_KEEP` | `14` | Archives kept |
| `CLAVREN_BACKUP_AGE_RECIPIENT` | (none) | An `age1…` public key: archives are encrypted to it |

Logs go to the journal: `journalctl -u clavren -f`. They never contain keys, passwords, tokens or
customer names and emails.

---

## Upgrading

1. Back up: `sudo systemctl start clavren-backup && systemctl status clavren-backup`.
2. Keep the old binaries: `sudo cp /usr/local/bin/clavren{,.previous}` (same for
   `clavren-admin`).
3. Install the new ones and restart: `sudo systemctl restart clavren`. Migrations run at start.
4. Check `systemctl status clavren` and the dashboard's Overview.

To roll back, put the `.previous` binaries back and restart. If the new version ran a migration
the old one doesn't know, restore the backup from step 1 as well.

---

## Changing the token-signing key

Applications trust token-signing keys by version (`kid`), embedded when they are built.

1. `sudo -u clavren … clavren-admin show-public-key` prints the key in use and the next one.
2. Ship an application version that trusts both (see
   [client-integration.md](client-integration.md#trusted-keys)).
3. Once every copy in use has updated, set `CLAVREN_SIGNING_KEY_VERSION` to the next version and
   restart. Tokens already issued stay valid until they expire; new ones use the new key.

Keys are derived from `master.key`; nothing new needs to be stored or backed up.
