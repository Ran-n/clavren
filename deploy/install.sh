#!/usr/bin/env bash
# ------------------------------------------------------------------------------
#+ Authors: 	Ran# <ran.hash@proton.me>
#+ Created: 	2026/11/03 14:36:22.589520
#+ Revised: 	2026/11/09 10:00:42.649060
# ------------------------------------------------------------------------------

# Installs or upgrades Clavren on a Debian-based server with systemd (Raspberry Pi OS Bookworm or
# newer). Run it as root from an unpacked release bundle (binaries in bin/) or from a source tree
# after `cargo build --release` (binaries in target/release/):
#
#   sudo ./deploy/install.sh --public-url https://licences.example.com
#
# First run: installs PostgreSQL, creates the database role with a random password, the clavren
# user and folders, /etc/clavren/clavren.env, the services, starts everything, makes the first
# backup and creates the first admin. Later runs upgrade: back up, keep the old binaries as
# *.previous, install the new ones and restart. Every step can be repeated safely; any failure
# stops the script with a message. docs/deployment.md describes the same steps by hand.
set -euo pipefail
umask 022

readonly ENV_FILE=/etc/clavren/clavren.env
readonly BIN_DIR=/usr/local/bin
readonly LIB_DIR=/usr/local/lib/clavren
readonly DOC_DIR=/usr/local/share/doc/clavren
readonly DATA_DIR=/var/lib/clavren
readonly UNIT_DIR=/etc/systemd/system
readonly DEFAULT_BACKUP_DIR=/mnt/clavren-backups

say() { printf 'clavren-install: %s\n' "$*"; }
warn() { printf 'clavren-install: WARNING: %s\n' "$*" >&2; }
fail() {
    printf 'clavren-install: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Usage: sudo ./deploy/install.sh [options]

  --public-url URL   The https:// address apps and the dashboard use (asked when missing;
                     first installation only)
  --backup-dir DIR   Where backups go (default /mnt/clavren-backups; first installation only)
  --admin NAME       Username of the first admin (asked when missing)
  --skip-backup      Upgrade without backing up first
  -h, --help         This text
EOF
}

public_url=''
backup_dir=''
admin_name=''
skip_backup=false
while (($#)); do
    case $1 in
        --public-url) public_url=${2:?--public-url needs a value}; shift 2 ;;
        --backup-dir) backup_dir=${2:?--backup-dir needs a value}; shift 2 ;;
        --admin) admin_name=${2:?--admin needs a value}; shift 2 ;;
        --skip-backup) skip_backup=true; shift ;;
        -h | --help) usage; exit 0 ;;
        *) usage >&2; fail "unknown option '$1'" ;;
    esac
done

interactive() { [[ -t 0 && -t 1 ]]; }

ask() {
    local prompt=$1 default=${2:-} answer
    interactive || fail "$prompt: no answer given and no terminal to ask on (see --help)"
    read -r -p "$prompt${default:+ [$default]}: " answer
    printf '%s' "${answer:-$default}"
}

# A backup directory counts as ready when it is on another drive than the system, as
# clavren-backup.sh demands.
backup_drive_mounted() { [[ -d $1 && $(stat -c %d "$1") != "$(stat -c %d /)" ]]; }

# clavren-admin with the server configuration, as the clavren user.
run_admin() {
    runuser -u clavren -- bash -c "set -a; . $ENV_FILE; set +a; exec $BIN_DIR/clavren-admin \"\$@\"" clavren-admin "$@"
}

# A variable from the configuration file, or the given default.
env_value() {
    # shellcheck source=/dev/null
    (. "$ENV_FILE"; printf '%s' "${!1:-$2}")
}

# --- Checks -------------------------------------------------------------------

((EUID == 0)) || fail "run it as root: sudo $0"
command -v apt-get >/dev/null || fail "apt-get not found: this script is for Debian-based systems"
command -v systemctl >/dev/null || fail "systemctl not found: this script needs systemd"

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
source_bin=''
for candidate in "$root/bin" "$root/target/release"; do
    if [[ -x $candidate/clavren && -x $candidate/clavren-admin ]]; then
        source_bin=$candidate
        break
    fi
done
[[ -n $source_bin ]] || fail "no clavren binaries in $root/bin or $root/target/release; build them first"
if ! output=$("$source_bin/clavren-admin" --help 2>&1); then
    fail "the binaries in $source_bin don't run on this $(uname -m) machine: $output"
fi
readonly UNITS=(clavren.service clavren-backup.service clavren-backup.timer
    clavren-update-check.service clavren-update-check.timer clavren-update-check.path
    clavren-update.service clavren-update.path)
for file in "${UNITS[@]}" clavren-backup.sh clavren-restore.sh clavren-new-admin.sh clavren-update-check.sh clavren-update.sh clavren.env.example; do
    [[ -f $root/deploy/$file ]] || fail "missing $root/deploy/$file"
done
# psql and friends complain when they start in a folder the postgres user cannot read.
cd /

if [[ -f $ENV_FILE ]]; then upgrading=true; else upgrading=false; fi

# --- First installation: packages, database, user, configuration ---------------

if ! $upgrading; then
    say "first installation"

    [[ -n $public_url ]] || public_url=$(ask "Public address (https://…)")
    public_url=${public_url%/}
    [[ $public_url =~ ^https://[^/?#[:space:]|]+$ || $public_url =~ ^http://(localhost|127\.0\.0\.1|\[::1\])(:[0-9]+)?$ ]] ||
        fail "the public address must be https:// and a host only, e.g. https://licences.example.com, not '$public_url'"

    [[ -n $backup_dir ]] || backup_dir=$DEFAULT_BACKUP_DIR
    backup_dir=${backup_dir%/}
    [[ $backup_dir =~ ^/[^[:space:]|]+$ ]] || fail "the backup directory must be an absolute path without spaces, not '$backup_dir'"

    say "installing packages"
    apt-get update -q
    DEBIAN_FRONTEND=noninteractive apt-get install -y -q postgresql postgresql-client b3sum age curl jq
    systemctl enable --now postgresql

    if ! id clavren >/dev/null 2>&1; then
        say "creating the clavren user"
        useradd --system --home-dir "$DATA_DIR" --shell /usr/sbin/nologin clavren
    fi

    # A new configuration file gets a new password: the role's old one, if any, is unknown here.
    say "creating the database"
    password=$(head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n')
    runuser -u postgres -- psql --quiet --no-psqlrc -v ON_ERROR_STOP=1 <<SQL
\set password '$password'
SELECT 'CREATE ROLE clavren LOGIN' WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'clavren') \gexec
ALTER ROLE clavren WITH LOGIN PASSWORD :'password';
SELECT 'CREATE DATABASE clavren OWNER clavren' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'clavren') \gexec
SQL

    say "writing $ENV_FILE"
    install -d -o root -g clavren -m 0750 /etc/clavren
    install -o root -g clavren -m 0640 /dev/null "$ENV_FILE.new"
    sed -e "s|^CLAVREN_DATABASE_URL=.*|CLAVREN_DATABASE_URL=postgres://clavren:$password@127.0.0.1:5432/clavren|" \
        -e "s|^CLAVREN_PUBLIC_URL=.*|CLAVREN_PUBLIC_URL=$public_url|" \
        -e "s|^CLAVREN_BACKUP_DIR=.*|CLAVREN_BACKUP_DIR=$backup_dir|" \
        "$root/deploy/clavren.env.example" >"$ENV_FILE.new"
    mv "$ENV_FILE.new" "$ENV_FILE"
else
    say "upgrading the installation configured in $ENV_FILE"
    id clavren >/dev/null 2>&1 || fail "$ENV_FILE exists but the clavren user doesn't; fix the installation by hand"
    [[ -z $public_url && -z $backup_dir ]] || warn "--public-url and --backup-dir only apply to a first installation; edit $ENV_FILE instead"
    backup_dir=$(env_value CLAVREN_BACKUP_DIR "$DEFAULT_BACKUP_DIR")

    # The update check and the updater read JSON with jq, which installations before 0.4 lack.
    if ! command -v jq >/dev/null; then
        say "installing jq"
        apt-get update -q
        DEBIAN_FRONTEND=noninteractive apt-get install -y -q jq
    fi

    if $skip_backup; then
        warn "upgrading without a backup (--skip-backup)"
    elif backup_drive_mounted "$backup_dir"; then
        say "backing up before the upgrade"
        systemctl start clavren-backup || fail "the backup failed (journalctl -u clavren-backup); fix it or pass --skip-backup"
    else
        fail "$backup_dir is not on a mounted backup drive, so nothing can be backed up first; mount it or pass --skip-backup"
    fi

    for binary in clavren clavren-admin; do
        if [[ -f $BIN_DIR/$binary ]]; then
            cp -p "$BIN_DIR/$binary" "$BIN_DIR/$binary.previous"
        fi
    done
    say "kept the old binaries as $BIN_DIR/clavren{,-admin}.previous"
fi

# --- Files ----------------------------------------------------------------------

say "installing files"
install -d -o clavren -g clavren -m 0700 "$DATA_DIR"
# Shared with the update scripts: the server writes requests here, the check its findings.
install -d -o clavren -g clavren -m 0700 "$DATA_DIR/updates"
install -d -o root -g clavren -m 0750 "$LIB_DIR"
install -m 0755 "$source_bin/clavren" "$source_bin/clavren-admin" "$BIN_DIR/"
install -m 0755 "$root/deploy/clavren-new-admin.sh" "$BIN_DIR/clavren-new-admin"
install -m 0750 -g clavren "$root/deploy/clavren-backup.sh" "$root/deploy/clavren-restore.sh"     "$root/deploy/clavren-update-check.sh" "$LIB_DIR/"
# The updater runs as root: only root may read or change it.
install -m 0700 "$root/deploy/clavren-update.sh" "$LIB_DIR/"
for unit in "${UNITS[@]}"; do
    install -m 0644 "$root/deploy/$unit" "$UNIT_DIR/"
done
if [[ $backup_dir != "$DEFAULT_BACKUP_DIR" ]]; then
    sed -i "s|$DEFAULT_BACKUP_DIR|$backup_dir|g" "$UNIT_DIR/clavren-backup.service"
fi
if [[ -d $root/docs ]]; then
    install -d -m 0755 "$DOC_DIR"
    install -m 0644 "$root"/docs/*.md "$DOC_DIR/"
fi

backup_ready=false
if backup_drive_mounted "$backup_dir"; then
    chown clavren:clavren "$backup_dir"
    chmod 0700 "$backup_dir"
    backup_ready=true
else
    mkdir -p "$backup_dir"
    warn "$backup_dir is not on its own drive yet: mount one there (nofail in /etc/fstab), then run
    sudo install -d -o clavren -g clavren -m 0700 $backup_dir
    sudo systemctl start clavren-backup"
fi

# --- Start ----------------------------------------------------------------------

say "starting clavren"
systemctl daemon-reload
systemctl enable clavren clavren-backup.timer clavren-update-check.timer clavren-update-check.path clavren-update.path
systemctl restart clavren
systemctl start clavren-backup.timer clavren-update-check.timer clavren-update-check.path clavren-update.path

listen=$(env_value CLAVREN_LISTEN 127.0.0.1:8080)
listen=${listen/#0.0.0.0:/127.0.0.1:}
listen=${listen/#\[::\]:/[::1]:}
ready=false
for _ in $(seq 30); do
    if curl -fs -o /dev/null "http://$listen/health/ready"; then
        ready=true
        break
    fi
    sleep 1
done
$ready || fail "clavren did not become ready on $listen within 30 seconds; see journalctl -u clavren -n 50"
say "clavren is running on $listen"

# --- First installation: backup and admin ---------------------------------------

if ! $upgrading; then
    if $backup_ready; then
        say "making the first backup"
        systemctl start clavren-backup || fail "the first backup failed; see journalctl -u clavren-backup"
        say "copy the archive in $backup_dir somewhere else: without master.key no licence key can be read"
    else
        warn "no backup made yet: $DATA_DIR/master.key exists only on this machine"
    fi
fi

admins=$(run_admin list-admins) || fail "clavren-admin list-admins failed"
if [[ $admins == USERNAME* ]]; then
    : # an admin exists already
elif interactive; then
    [[ -n $admin_name ]] || admin_name=$(ask "Username of the first admin" "${SUDO_USER:-admin}")
    say "creating admin $admin_name: have your authenticator app ready"
    run_admin create-admin "$admin_name"
else
    warn "there is no admin yet; in a terminal on this machine, run clavren-new-admin
    (from another machine: ssh -t <this machine> clavren-new-admin)"
fi

if $upgrading; then
    say "upgrade done. To roll back: put $BIN_DIR/clavren{,-admin}.previous back and restart clavren"
else
    say "installation done. Next: put HTTPS in front of $listen (Cloudflare Tunnel or a reverse proxy,
    see $DOC_DIR/deployment.md) so that $(env_value CLAVREN_PUBLIC_URL '') reaches it"
fi
