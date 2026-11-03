[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/03 04:54:17.627908 )
[//]: # (+ Revised: 	2026/11/03 15:25:07.576642 )
[//]: # ( ---------------------------------------------------------------------- )

# Deployment

This guide puts Clavren on a server for real use: a Raspberry Pi (or another arm64 Debian machine)
running Clavren as a service, with its database, nightly backups and HTTPS. No Docker.

Follow the steps in order. Every step ends with a **Check** so you know it worked before moving on.

> [!IMPORTANT]
> `cargo dev` is not a way to run a server. It uses a throwaway database inside `target/`, keeps
> the master key where `cargo clean` deletes it, and stops when you close the terminal. It is only
> for working on Clavren ([development.md](development.md)).

**Contents**

1. [Before you start](#before-you-start), including [a fixed address for the Pi](#give-the-pi-a-fixed-address)
2. [Step 1: Get Clavren](#step-1-get-clavren)
3. [Step 2: Prepare the backup drive](#step-2-prepare-the-backup-drive)
4. [Step 3: Install](#step-3-install): [with the script](#option-a-with-the-script) or [by hand](#option-b-by-hand)
5. [Step 4: HTTPS](#step-4-https)
6. [Step 5: Final checks](#step-5-final-checks)
7. Afterwards: [Day to day](#day-to-day) · [Upgrading](#upgrading) · [Configuration](#configuration) · [Troubleshooting](#troubleshooting) · [Changing the public address](#changing-the-public-address) · [Changing the token-signing key](#changing-the-token-signing-key)

---

## Before you start

You need:

- **A Raspberry Pi 3 B+ or newer** with a 64-bit OS: Raspberry Pi OS Bookworm or newer, or
  Debian 12 or newer. You reach it over SSH; every step works from an SSH session, with no screen
  or browser on the Pi. Any other arm64 Debian-based system with systemd works too; for other
  processors (x86_64), you build from source in step 1. 1 GB of memory is plenty: Clavren,
  PostgreSQL and `cloudflared` together use about 200 MB.
- **A domain name** for Clavren, e.g. `licences.example.com`. Apps and the dashboard use it, and
  it is written into every licence token, so choose one you will keep.
- **A way to serve HTTPS**: a free Cloudflare account with your domain on it (easiest, no ports to
  open), or a reverse proxy such as Caddy.
- **A USB drive** for backups (a network share also works). Backups never go on the SD card.
- **An authenticator app** on your phone (Aegis, Google Authenticator, 1Password…). Every admin
  signs in with a password and a code from it.

In the commands below, replace `licences.example.com` with your domain, `your-pi` with your Pi's
address, and `X.Y.Z` with the version you install.

### Give the Pi a fixed address

Your router gives the Pi an address that can change, and then SSH stops finding it. Fix it on the
Pi. Find the connection's name, current address and router first:

```sh
nmcli -t -f NAME,DEVICE connection show --active    # e.g. "Wired connection 1:eth0"
ip -br addr show eth0                                # e.g. 192.168.1.50/24
ip route | head -1                                   # e.g. "default via 192.168.1.1"
```

Then keep that address for good (with your connection name, address and router):

```sh
sudo nmcli connection modify 'Wired connection 1' ipv4.method manual \
    ipv4.addresses 192.168.1.50/24 ipv4.gateway 192.168.1.1 ipv4.dns 192.168.1.1
sudo systemd-run nmcli connection up 'Wired connection 1'
```

`systemd-run` lets the change finish even though it briefly drops your SSH connection. Also
reserve that address for the Pi in your router's DHCP settings (or check it is outside the range
the router hands out), so no other device ever gets it.

**Check**: after a few seconds, SSH to the Pi again;
`nmcli -g ipv4.method connection show 'Wired connection 1'` prints `manual`.

---

## Step 1: Get Clavren

Choose **one**.

### From a release (recommended)

1. On the Pi, download `clavren-X.Y.Z-aarch64-linux.tar.gz` and `clavren-X.Y.Z-aarch64-linux.tar.gz.b3`
   from the [latest release](https://github.com/Ran-n/clavren/releases/latest) (also on GitLab and
   Codeberg).
2. Check and unpack it:

   ```sh
   sudo apt install b3sum
   b3sum -c clavren-X.Y.Z-aarch64-linux.tar.gz.b3
   tar xzf clavren-X.Y.Z-aarch64-linux.tar.gz
   cd clavren-X.Y.Z-aarch64-linux
   ```

**Check**: `b3sum` printed `OK`, and `./bin/clavren-admin --help` lists the commands. If it
says `Exec format error` or `GLIBC_… not found`, this machine can't run the release: build from
source instead.

### From source

Needs [Rust](https://rustup.rs/) and [Node.js](https://nodejs.org/) 22 or newer on the server.

```sh
git clone https://github.com/Ran-n/clavren
cd clavren
cd web/admin && npm ci && npm run build && cd ../..                # the dashboard first: the server embeds it
cargo build --release --bin clavren --bin clavren-admin           # takes a while on a Pi
```

**Check**: `./target/release/clavren-admin --help` lists the commands.

The by-hand steps below use `bin/clavren` and `bin/clavren-admin`. From source, they are
`target/release/clavren` and `target/release/clavren-admin` instead.

---

## Step 2: Prepare the backup drive

Every backup holds the database and `master.key`, the key that unlocks every licence key. Losing
both means every customer needs a new key, so backups go on a separate drive.

1. Plug in the USB drive and find its UUID and filesystem type:

   ```sh
   lsblk -f
   ```

2. Mount it at `/mnt/clavren-backups` on every boot. `nofail` lets the Pi start even when the drive
   is missing. Put your UUID and type (e.g. `ext4`) in:

   ```sh
   sudo mkdir -p /mnt/clavren-backups
   echo 'UUID=your-uuid  /mnt/clavren-backups  ext4  defaults,nofail  0  2' | sudo tee -a /etc/fstab
   sudo systemctl daemon-reload
   sudo mount /mnt/clavren-backups
   ```

**Check**: `findmnt /mnt/clavren-backups` shows your drive.

No drive yet? The install still works, but no backup runs until there is one. Until then, make a
backup by hand and keep it on another computer: see
[backup-and-recovery.md](backup-and-recovery.md#no-backup-drive-yet).

To use another folder, pass it to the install script with `--backup-dir`, or see
[Configuration](#configuration) when installing by hand. You can also skip this step for now: the
install works without it, but makes no backup until the drive is there.

---

## Step 3: Install

Choose **one**. Both end in the same place.

### Option A: with the script

From the folder of step 1:

```sh
sudo ./deploy/install.sh --public-url https://licences.example.com
```

It asks for a username for the first admin, then:

1. asks for a password (twice);
2. shows a QR code: scan it with your authenticator app;
3. asks for the code the app shows;
4. prints 10 recovery codes. **Write them down and keep them offline**: they are the only way in if
   you lose your phone.

Everything else happens on its own: PostgreSQL, the database with a random password, the
`clavren` user, the configuration file, the services, the first backup.

**Check**: the script ends with `installation done`. If it stops earlier, its last line says why;
see [Troubleshooting](#troubleshooting).

Run without a terminal (e.g. `ssh your-pi 'sudo ./deploy/install.sh …'`, without `-t`), it does
everything except the admin and says so at the end. Create the admin afterwards as in
[B8](#b8-create-the-first-admin).

| Option | Meaning |
|--------|---------|
| `--public-url URL` | Your `https://` address (asked for when missing) |
| `--admin NAME` | The first admin's username (asked for when missing) |
| `--backup-dir DIR` | The backup folder, if not `/mnt/clavren-backups` |
| `--skip-backup` | When upgrading: don't back up first |

Now go to [Step 4](#step-4-https).

### Option B: by hand

The same as the script, one command at a time. Run them from the folder of step 1.

#### B1. Install PostgreSQL and the tools

```sh
sudo apt update
sudo apt install postgresql postgresql-client b3sum age curl
```

**Check**: `systemctl is-active postgresql` prints `active`.

PostgreSQL only listens on the Pi itself by default. Leave it that way.

#### B2. Create the database

Make a random password and copy it; you need it twice now and once in B4:

```sh
openssl rand -hex 24
```

Create the database user (paste the password when asked, twice) and the database:

```sh
sudo -u postgres createuser --pwprompt clavren
sudo -u postgres createdb --owner clavren clavren
```

**Check**: `psql postgres://clavren:PASSWORD@127.0.0.1:5432/clavren -c 'select 1'` (with your
password) prints a `1`.

#### B3. Create the `clavren` user and its folders

Clavren runs as its own user, with no login and no home folder of yours:

```sh
sudo useradd --system --home-dir /var/lib/clavren --shell /usr/sbin/nologin clavren
sudo install -d -o clavren -g clavren -m 0700 /var/lib/clavren
sudo install -d -o root -g clavren -m 0750 /etc/clavren /usr/local/lib/clavren
```

**Check**: `id clavren` shows the user.

#### B4. Write the configuration

```sh
sudo install -m 0640 -g clavren deploy/clavren.env.example /etc/clavren/clavren.env
sudo nano /etc/clavren/clavren.env
```

Change these three lines (keep the rest as it is):

```sh
CLAVREN_DATABASE_URL=postgres://clavren:THE-PASSWORD-FROM-B2@127.0.0.1:5432/clavren
CLAVREN_PUBLIC_URL=https://licences.example.com
CLAVREN_BACKUP_DIR=/mnt/clavren-backups
```

Save with Ctrl+O, Enter, and leave with Ctrl+X.

**Check**: `sudo grep -E '^CLAVREN_(DATABASE|PUBLIC)_URL' /etc/clavren/clavren.env` shows your
values.

#### B5. Install the programs and services

```sh
sudo install -m 0755 bin/clavren bin/clavren-admin /usr/local/bin/
sudo install -m 0755 deploy/clavren-new-admin.sh /usr/local/bin/clavren-new-admin
sudo install -m 0750 -g clavren deploy/clavren-backup.sh deploy/clavren-restore.sh /usr/local/lib/clavren/
sudo install -m 0644 deploy/clavren.service deploy/clavren-backup.service deploy/clavren-backup.timer /etc/systemd/system/
sudo install -d -m 0755 /usr/local/share/doc/clavren
sudo install -m 0644 docs/*.md /usr/local/share/doc/clavren/
```

**Check**: `clavren-admin --help` lists the commands.

#### B6. Start Clavren

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now clavren clavren-backup.timer
```

The first start creates `/var/lib/clavren/master.key` and sets up the database tables.

**Check**: `curl -s http://127.0.0.1:8080/health/ready` prints `{"status":"ok"}`. If not,
`journalctl -u clavren -n 50` says why.

#### B7. Make the first backup

Give the backup drive to `clavren`, then back up:

```sh
sudo install -d -o clavren -g clavren -m 0700 /mnt/clavren-backups
sudo systemctl start clavren-backup
```

**Check**: `ls /mnt/clavren-backups` shows a `clavren-….tar` and its `.b3`.

#### B8. Create the first admin

Maximise your terminal window, then run:

```sh
clavren-new-admin
```

or, from your own computer, `ssh -t your-pi clavren-new-admin`. It works on the terminal's
alternate screen (like `less` or `nano`), so nothing it shows stays in your scrollback:

1. it asks for a username (lowercase letters, digits, `.`, `_`, `-`);
2. a password, twice (never shown);
3. shows a QR code: scan it with your authenticator app, then type the code the app shows;
4. prints 10 recovery codes. **Write them down and keep them offline**: they are the only way in
   if you lose your phone;
5. waits for Enter, then wipes the screen.

For the other admin commands (`list-admins`, `reset-password`…), add this shortcut to your
`~/.bashrc` once; it runs `clavren-admin` as the `clavren` user with the configuration loaded:

```sh
cat >>~/.bashrc <<'EOF'
clavren-admin() {
    sudo runuser -u clavren -- bash -c 'set -a; . /etc/clavren/clavren.env; exec /usr/local/bin/clavren-admin "$@"' clavren-admin "$@"
}
EOF
source ~/.bashrc
```

**Check**: `clavren-admin list-admins` shows your admin.

---

## Step 4: HTTPS

Clavren only answers on the Pi itself (`127.0.0.1:8080`). This step makes it reachable at your
domain over HTTPS. Choose **one**.

### Cloudflare Tunnel (recommended: no ports to open)

Your domain must be on your Cloudflare account. Everything runs on the Pi, over SSH.

1. Install `cloudflared` from Cloudflare's package repository:

   ```sh
   sudo mkdir -p --mode=0755 /usr/share/keyrings
   curl -fsSL https://pkg.cloudflare.com/cloudflare-main.gpg | sudo tee /usr/share/keyrings/cloudflare-main.gpg >/dev/null
   echo 'deb [signed-by=/usr/share/keyrings/cloudflare-main.gpg] https://pkg.cloudflare.com/cloudflared any main' | sudo tee /etc/apt/sources.list.d/cloudflared.list
   sudo apt update && sudo apt install cloudflared
   ```

2. Connect it to your Cloudflare account:

   ```sh
   cloudflared tunnel login
   ```

   It prints a link. Open it in a browser on **any** device (your computer, your phone), sign in
   to Cloudflare, pick your domain and click **Authorize**. The command on the Pi then finishes by
   itself.

3. Create the tunnel and point your hostname at it:

   ```sh
   cloudflared tunnel create clavren
   cloudflared tunnel route dns clavren licences.example.com
   ```

   The first command prints the tunnel's ID (a long `xxxxxxxx-xxxx-…` value); you need it next.

4. Write the tunnel's configuration (with your ID and hostname) and start it as a service:

   ```sh
   ID=the-tunnel-id
   sudo install -d -m 0755 /etc/cloudflared
   sudo install -m 0600 ~/.cloudflared/$ID.json /etc/cloudflared/
   sudo tee /etc/cloudflared/config.yml <<EOF
   tunnel: $ID
   credentials-file: /etc/cloudflared/$ID.json
   ingress:
     - hostname: licences.example.com
       service: http://127.0.0.1:8080
     - service: http_status:404
   EOF
   sudo cloudflared tunnel ingress validate --config /etc/cloudflared/config.yml
   sudo cloudflared service install
   ```

Nothing to change in Clavren: it already trusts `cloudflared` on the Pi.

Steps 2 to 4 can also be done in the Cloudflare dashboard: **Zero Trust → Networks → Tunnels →
Create a tunnel**, run the command it shows on the Pi, and add a public hostname with service
**HTTP** and URL **`127.0.0.1:8080`**.

Other services on the same Pi can share this tunnel: one more `hostname` rule each, above the
last `http_status:404` line, and `cloudflared tunnel route dns clavren` for their hostnames.

### Caddy (or another reverse proxy)

Your router must forward ports 80 and 443 to the Pi, and your domain must point at your public IP.

1. Install Caddy (`sudo apt install caddy`) and put this in `/etc/caddy/Caddyfile`:

   ```text
   licences.example.com {
       reverse_proxy 127.0.0.1:8080
   }
   ```

2. Tell Clavren where Caddy puts the visitor's address. Add this line to
   `/etc/clavren/clavren.env`:

   ```sh
   CLAVREN_CLIENT_IP_HEADER=x-forwarded-for
   ```

3. Restart both: `sudo systemctl restart caddy clavren`.

Skipping step 2 makes every visitor look like the same one, so one person's failed sign-ins
would lock everyone out.

**Check** (either way): from any computer,
`curl -s https://licences.example.com/health/ready` prints `{"status":"ok"}`.

---

## Step 5: Final checks

1. Open `https://licences.example.com` and sign in with your admin.
2. The **Overview** shows the last backup (or warns there is none).
3. **Copy the first backup off the Pi.** Only `clavren` can read the backups, so first copy them
   to your home folder on the Pi:

   ```sh
   sudo cp /mnt/clavren-backups/clavren-* ~ && sudo chown "$USER" ~/clavren-*
   ```

   then, on your computer (with your Pi's user and address):

   ```sh
   scp 'pi@your-pi:clavren-*' .
   ```

   and delete the copies on the Pi (`rm ~/clavren-*`). A drive plugged into the Pi doesn't survive
   a fire or a theft, and without `master.key` nothing can be recovered. To encrypt backups, see
   [backup-and-recovery.md](backup-and-recovery.md).

You're done. The dashboard only works at the public `https://` address: its sign-in cookie is
HTTPS-only.

---

## Day to day

| Task | Command |
|------|---------|
| Is it running? | `systemctl status clavren` |
| Watch the logs | `journalctl -u clavren -f` |
| Restart | `sudo systemctl restart clavren` |
| Back up now | `sudo systemctl start clavren-backup` |
| See recent backups | `journalctl -u clavren-backup -n 20`, or the dashboard's Backups page |
| List admins | `clavren-admin list-admins` |
| Add an admin | `clavren-new-admin` (nothing stays on screen), or `clavren-admin create-admin NAME` |
| Forgotten password or lost phone | `clavren-admin reset-password NAME` (new password, new authenticator, new recovery codes) |
| Block an admin | `clavren-admin disable-admin NAME` (`enable-admin` undoes it) |

`clavren-admin` here is the shortcut from [B8](#b8-create-the-first-admin); add it to your
`~/.bashrc` even if you used the script. All commands are in [api.md](api.md#clavren-admin).
Logs never contain keys, passwords, tokens, or customer names and emails.

Backups run every night at about 03:15 and keep the last 14.

---

## Upgrading

### With the script

Unpack the new release (or `git pull` and rebuild, as in step 1), then from its folder:

```sh
sudo ./deploy/install.sh
```

It backs up first, keeps the old programs as `/usr/local/bin/clavren.previous` and
`clavren-admin.previous`, installs the new ones and restarts.

### By hand

From the new release's folder:

```sh
sudo systemctl start clavren-backup                                   # 1. back up
sudo cp /usr/local/bin/clavren /usr/local/bin/clavren.previous        # 2. keep the old programs
sudo cp /usr/local/bin/clavren-admin /usr/local/bin/clavren-admin.previous
sudo install -m 0755 bin/clavren bin/clavren-admin /usr/local/bin/   # 3. install the new ones
sudo install -m 0755 deploy/clavren-new-admin.sh /usr/local/bin/clavren-new-admin
sudo install -m 0750 -g clavren deploy/clavren-backup.sh deploy/clavren-restore.sh /usr/local/lib/clavren/
sudo systemctl restart clavren                                        # 4. restart
```

**Check** (either way): `curl -s http://127.0.0.1:8080/health/ready` prints `{"status":"ok"}`,
and the dashboard's Overview looks right.

### Going back

```sh
sudo cp /usr/local/bin/clavren.previous /usr/local/bin/clavren
sudo cp /usr/local/bin/clavren-admin.previous /usr/local/bin/clavren-admin
sudo systemctl restart clavren
```

If the old version then refuses to start, the new one changed the database: restore the backup
made at the start of the upgrade as well ([backup-and-recovery.md](backup-and-recovery.md#restoring)).

---

## Configuration

All settings live in `/etc/clavren/clavren.env`. After changing it, run
`sudo systemctl restart clavren`.

| Setting | Default | Meaning |
|---------|---------|---------|
| `CLAVREN_DATABASE_URL` | (required) | `postgres://clavren:password@127.0.0.1:5432/clavren` |
| `CLAVREN_PUBLIC_URL` | (required) | Your `https://` address |
| `CLAVREN_LISTEN` | `127.0.0.1:8080` | Address and port Clavren listens on |
| `CLAVREN_DATA_DIR` | `/var/lib/clavren` | Where `master.key` lives |
| `CLAVREN_LOG` | `info` | How much to log, e.g. `debug` or `info,clavren=debug` |
| `CLAVREN_TRUSTED_PROXIES` | `127.0.0.1,::1` | Proxies allowed to tell Clavren the visitor's address, or `none` |
| `CLAVREN_CLIENT_IP_HEADER` | `cf-connecting-ip` | The header they use (`x-forwarded-for` for Caddy and most proxies) |
| `CLAVREN_SESSION_IDLE_HOURS` | `12` | Admins are signed out after this long unused |
| `CLAVREN_SESSION_MAX_DAYS` | `7` | Admins are signed out this long after signing in |
| `CLAVREN_LOGIN_MAX_FAILURES` | `5` | Failed sign-ins allowed per address and per username… |
| `CLAVREN_LOGIN_WINDOW_MINUTES` | `15` | …within this many minutes |
| `CLAVREN_TOKEN_DAYS` | `30` | Days an app keeps working offline after its last check |
| `CLAVREN_SIGNING_KEY_VERSION` | `1` | Token-signing key in use ([below](#changing-the-token-signing-key)) |
| `CLAVREN_BACKUP_DIR` | (required for backups) | The backup drive |
| `CLAVREN_BACKUP_KEEP` | `14` | How many backups to keep |
| `CLAVREN_BACKUP_AGE_RECIPIENT` | (none) | An `age1…` public key: backups are encrypted to it |

**Another backup folder**: change `CLAVREN_BACKUP_DIR`, and change both `/mnt/clavren-backups`
paths in `/etc/systemd/system/clavren-backup.service` to match (the install script does this for
`--backup-dir`). Then `sudo systemctl daemon-reload`.

### Where everything is

| What | Where |
|------|-------|
| The server (dashboard built in) | `/usr/local/bin/clavren`, run by `clavren.service` as the `clavren` user |
| The admin tools | `/usr/local/bin/clavren-admin`, and `/usr/local/bin/clavren-new-admin` (creates an admin on the alternate screen) |
| Settings | `/etc/clavren/clavren.env` (root:clavren, 0640) |
| `master.key` | `/var/lib/clavren/master.key` (clavren, 0600) |
| Database | PostgreSQL from `apt`, database and user `clavren`, localhost only |
| Backup and restore scripts | `/usr/local/lib/clavren/` |
| Services | `/etc/systemd/system/clavren.service`, `clavren-backup.service`, `clavren-backup.timer` |
| Backups | `CLAVREN_BACKUP_DIR`, nightly at about 03:15 |
| These guides | `/usr/local/share/doc/clavren/` |

---

## Troubleshooting

| What you see | What to do |
|--------------|------------|
| `Exec format error`, `cannot execute binary file` or `GLIBC_… not found` | The release doesn't fit this machine. [Build from source](#from-source) |
| `/health/ready` doesn't answer, or the script says Clavren did not become ready | Run `journalctl -u clavren -n 50`. A settings mistake names the setting. `password authentication failed` means the password in `CLAVREN_DATABASE_URL` isn't the one from B2 |
| `is on the system drive; is the backup drive mounted?` | The backup drive isn't mounted: `sudo mount /mnt/clavren-backups`, and check the line in `/etc/fstab` |
| The dashboard signs in, then shows the sign-in page again | Open it at your `https://` address, not the Pi's IP |
| Everyone gets "too many attempts" at once | The proxy's visitor-address header isn't set: [Caddy](#caddy-or-another-reverse-proxy) step 2 |
| Clavren won't start because `master.key` is missing | Restore a backup ([backup-and-recovery.md](backup-and-recovery.md#restoring)). Clavren never makes a new key for a database that has data |
| An admin lost their phone | Sign in with a recovery code and set up the new phone under **Account**, or `clavren-admin reset-password NAME` |

---

## Changing the public address

Change it before any app has activated if you can: the address is written into every token as its
issuer. (The client library doesn't check it, so a change later still works.)

1. Point the new hostname at the tunnel and serve it (or change the hostname in your proxy):

   ```sh
   cloudflared tunnel route dns clavren new.example.com
   sudo sed -i 's|hostname: licences.example.com|hostname: new.example.com|' /etc/cloudflared/config.yml
   ```

2. Tell Clavren, and restart both:

   ```sh
   sudo sed -i 's|^CLAVREN_PUBLIC_URL=.*|CLAVREN_PUBLIC_URL=https://new.example.com|' /etc/clavren/clavren.env
   sudo systemctl restart clavren cloudflared
   ```

3. Delete the old hostname's DNS record in the Cloudflare dashboard (**your domain → DNS →
   Records**): `cloudflared` can add records but not delete them. Until then the tunnel answers
   404 for it.

**Check**: `curl -s https://new.example.com/health/ready` prints `{"status":"ok"}`. Admins sign in
again at the new address (sessions belong to the old one); their authenticator entries, named
"Clavren", keep working. Apps you build from now on use the new address.

---

## Changing the token-signing key

Apps check licence tokens with a public key built into each app version. To switch to a new key
without locking anyone out:

1. `clavren-admin show-public-key` prints the key in use and the next one.
2. Release an app version that trusts both
   ([client-integration.md](client-integration.md#trusted-keys)).
3. Once everyone has that version, set `CLAVREN_SIGNING_KEY_VERSION` to the next number and
   restart. Tokens already issued keep working until they expire.

The keys come from `master.key`; there is nothing new to store or back up.
