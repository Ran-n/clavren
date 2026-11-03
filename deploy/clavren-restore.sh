#!/usr/bin/env bash
# ------------------------------------------------------------------------------
#+ Authors: 	Ran# <ran.hash@proton.me>
#+ Created: 	2026/11/03 05:07:51.919967
#+ Revised: 	2026/11/03 05:08:00.356768
# ------------------------------------------------------------------------------

# Restores a Clavren backup made by clavren-backup.sh onto this machine: checks the BLAKE3
# checksum, decrypts it if it is an age archive, stops Clavren, replaces the database contents and
# master.key, and starts Clavren again. Run as root; see docs/backup-and-recovery.md.
#   clavren-restore.sh /mnt/clavren-backups/clavren-20270115T031502Z.tar
#   clavren-restore.sh archive.tar.age ~/age-identity.txt
set -euo pipefail
umask 077

fail() {
    printf 'clavren-restore: %s\n' "$*" >&2
    exit 1
}

[[ $EUID -eq 0 ]] || fail "run as root (it stops the service and writes /var/lib/clavren)"
archive=${1:?usage: clavren-restore.sh ARCHIVE [AGE_IDENTITY_FILE]}
identity=${2:-}
env_file=${CLAVREN_ENV_FILE:-/etc/clavren/clavren.env}
[[ -f $archive ]] || fail "$archive not found"
[[ -f $archive.b3 ]] || fail "$archive.b3 (its checksum) not found"

# shellcheck source=clavren.env.example
set -a && . "$env_file" && set +a
database_url=${CLAVREN_DATABASE_URL:?CLAVREN_DATABASE_URL is not set in $env_file}
data_dir=${CLAVREN_DATA_DIR:-/var/lib/clavren}

(cd "$(dirname "$archive")" && b3sum --check --quiet "$(basename "$archive").b3") || fail "checksum mismatch: the archive is damaged"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
tar_file=$archive
if [[ $archive == *.age ]]; then
    [[ -n $identity ]] || fail "$archive is encrypted: pass the age identity file as the second argument"
    age --decrypt --identity "$identity" --output "$work/backup.tar" "$archive"
    tar_file=$work/backup.tar
fi
tar -C "$work" -xf "$tar_file" clavren.dump master.key

systemctl stop clavren
pg_restore --clean --if-exists --no-owner --exit-on-error --single-transaction --dbname="$database_url" "$work/clavren.dump"
install -d -o clavren -g clavren -m 0700 "$data_dir"
install -o clavren -g clavren -m 0600 "$work/master.key" "$data_dir/master.key"
systemctl start clavren

printf 'clavren-restore: restored %s; check `systemctl status clavren` and the Overview\n' "$(basename "$archive")"
