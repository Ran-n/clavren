#!/usr/bin/env bash
# ------------------------------------------------------------------------------
#+ Authors: 	Ran# <ran.hash@proton.me>
#+ Created: 	2026/11/03 05:07:51.858253
#+ Revised: 	2026/11/03 15:25:05.902934
# ------------------------------------------------------------------------------

# One Clavren backup: a pg_dump (custom format) and master.key in one tar archive, optionally
# encrypted with age, plus its BLAKE3 checksum, written to CLAVREN_BACKUP_DIR. The two are useless
# without each other, so they always travel together. Refuses to run when the backup directory is
# on the same drive as the system, so a missing USB drive never fills the SD card. Keeps the last
# CLAVREN_BACKUP_KEEP archives and records each success in backup_runs (shown on the Overview).
# Any failure exits non-zero: `systemctl --failed` and the journal show it.
# Needs: pg_dump, psql (postgresql-client), b3sum, tar; age when CLAVREN_BACKUP_AGE_RECIPIENT is set.
set -euo pipefail
umask 077
# Run by hand from a home folder the clavren user can't read, find would warn on the way out.
cd /

fail() {
    printf 'clavren-backup: %s\n' "$*" >&2
    exit 1
}

database_url=${CLAVREN_DATABASE_URL:?CLAVREN_DATABASE_URL is not set}
backup_dir=${CLAVREN_BACKUP_DIR:?CLAVREN_BACKUP_DIR is not set}
data_dir=${CLAVREN_DATA_DIR:-/var/lib/clavren}
keep=${CLAVREN_BACKUP_KEEP:-14}
recipient=${CLAVREN_BACKUP_AGE_RECIPIENT:-}

[[ $keep =~ ^[1-9][0-9]*$ ]] || fail "CLAVREN_BACKUP_KEEP must be a whole number above 0, not '$keep'"
[[ -d $backup_dir && -w $backup_dir ]] || fail "$backup_dir is not a writable directory"
if [[ $(stat -c %d "$backup_dir") == "$(stat -c %d /)" ]]; then
    fail "$backup_dir is on the system drive; is the backup drive mounted?"
fi
[[ -r $data_dir/master.key ]] || fail "cannot read $data_dir/master.key"

name="clavren-$(date -u +%Y%m%dT%H%M%SZ).tar"
# Work on the backup drive: nothing is written to the SD card, and the final move is atomic.
work=$(mktemp -d "$backup_dir/.work.XXXXXX")
trap 'rm -rf "$work"' EXIT

pg_dump --format=custom --dbname="$database_url" --file="$work/clavren.dump"
cp "$data_dir/master.key" "$work/master.key"
tar -C "$work" -cf "$work/$name" clavren.dump master.key
if [[ -n $recipient ]]; then
    age --encrypt --recipient "$recipient" --output "$work/$name.age" "$work/$name"
    name="$name.age"
fi

(cd "$work" && b3sum "$name" >"$name.b3" && b3sum --check --quiet "$name.b3")
mv "$work/$name.b3" "$backup_dir/$name.b3"
mv "$work/$name" "$backup_dir/$name"
size=$(stat -c %s "$backup_dir/$name")

# psql variables (:'name') are quoted by psql itself; they only work in SQL read from stdin.
psql --dbname="$database_url" --quiet --no-psqlrc -v ON_ERROR_STOP=1 -v name="$name" -v size="$size" <<'SQL'
INSERT INTO backup_runs (archive_name, size_bytes) VALUES (:'name', :'size'::bigint);
SQL

# Archive names sort by time; keep the newest $keep.
mapfile -t archives < <(find "$backup_dir" -maxdepth 1 -type f -name 'clavren-*.tar*' ! -name '*.b3' -printf '%f\n' | sort -r)
for old in "${archives[@]:keep}"; do
    rm -f -- "$backup_dir/$old" "$backup_dir/$old.b3"
done

printf 'clavren-backup: wrote %s (%s bytes); keeping %s\n' "$name" "$size" "$keep"
