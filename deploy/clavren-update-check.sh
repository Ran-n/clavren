#!/usr/bin/env bash
# ------------------------------------------------------------------------------
#+ Authors: 	Ran# <ran.hash@proton.me>
#+ Created: 	2026/11/09 09:57:45.921632
#+ Revised: 	2026/11/09 11:10:33.678766
# ------------------------------------------------------------------------------

# Checks whether a newer Clavren release is out and writes what it found to
# CLAVREN_DATA_DIR/updates/latest.json, which the dashboard shows. Started by
# clavren-update-check.timer (5 minutes after boot, then every 6 hours) and by
# clavren-update-check.path when the dashboard's Check now writes updates/check-request.
#
# CLAVREN_UPDATE_SOURCE is a "latest release" address in GitHub's format (default: GitHub's API
# for Ran-n/clavren; Forgejo's /api/v1/repos/{owner}/{repo}/releases/latest answers the same way);
# `off` turns checking off. A release is installable only when its bundle and the bundle's .b3
# checksum are both uploaded. A failed check keeps the last good result, records why in plain
# English and exits non-zero, so `systemctl --failed` shows it.
# Needs: curl, jq.
set -euo pipefail
umask 077
cd /

readonly DEFAULT_SOURCE=https://api.github.com/repos/Ran-n/clavren/releases/latest

say() { printf 'clavren-update-check: %s\n' "$*"; }
fail() {
    printf 'clavren-update-check: %s\n' "$*" >&2
    exit 1
}

source_url=${CLAVREN_UPDATE_SOURCE:-$DEFAULT_SOURCE}
data_dir=${CLAVREN_DATA_DIR:-/var/lib/clavren}
backup_dir=${CLAVREN_BACKUP_DIR:-/mnt/clavren-backups}
updates_dir=$data_dir/updates
latest=$updates_dir/latest.json

# A check request is answered by this run, whatever its outcome.
rm -f -- "$updates_dir/check-request"

if [[ ${source_url,,} == off ]]; then
    say "checking for updates is off (CLAVREN_UPDATE_SOURCE=off)"
    exit 0
fi
[[ $source_url =~ ^https://[^[:space:]]+$ ]] || fail "CLAVREN_UPDATE_SOURCE must be off or an https:// address, not '$source_url'"
host=$(sed -E 's|^[a-z]+://([^/:]+).*|\1|' <<<"$source_url")

# The same test as backup_drive_mounted in install.sh: the updater needs it to back up first.
if [[ -d $backup_dir && $(stat -c %d "$backup_dir") != "$(stat -c %d /)" ]]; then
    backup_drive_mounted=true
else
    backup_drive_mounted=false
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
now=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# Writes latest.json in one step: the server sees the whole file or the previous one.
publish() {
    mkdir -p "$updates_dir"
    jq . >"$latest.new"
    mv -f "$latest.new" "$latest"
}

# Keeps the last good release, records why this check failed, and fails the unit.
failed() {
    local reason=$1 previous='{}'
    if [[ -r $latest ]] && jq -e 'type == "object"' "$latest" >/dev/null 2>&1; then
        previous=$(cat "$latest")
    fi
    jq --arg checked_at "$now" --arg error "$reason" --argjson mounted "$backup_drive_mounted" '{
            checked_at: $checked_at,
            version: .version, release_url: .release_url, published_at: .published_at,
            installable: (.installable // false), bundle_url: .bundle_url, checksum_url: .checksum_url,
            backup_drive_mounted: $mounted,
            error: $error
        }' <<<"$previous" | publish
    fail "$reason"
}

curl_error=0
status=$(curl -sS --proto =https --max-time 30 -o "$work/release.json" -w '%{http_code}' \
    -H 'Accept: application/vnd.github+json' -H 'User-Agent: clavren-update-check' \
    "$source_url" 2>"$work/curl.err") || curl_error=$?
case $curl_error in
    0) ;;
    6) failed "could not find $host (no network or DNS)" ;;
    7) failed "could not reach $host" ;;
    28) failed "$host did not answer within 30 seconds" ;;
    35 | 51 | 58 | 60) failed "the secure connection to $host failed" ;;
    *) failed "could not reach $host (curl error $curl_error)" ;;
esac
case $status in
    200) ;;
    403 | 429) failed "$host refused the check for now (rate limit); the next check tries again" ;;
    404) failed "$host has no release at $source_url" ;;
    *) failed "$host answered with HTTP $status" ;;
esac

tag=$(jq -r 'if type == "object" and (.tag_name | type) == "string" then .tag_name else "" end' "$work/release.json" 2>/dev/null) ||
    failed "the answer from $host is not a release"
[[ -n $tag ]] || failed "the answer from $host is not a release"
[[ $tag =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] ||
    failed "the latest release on $host has the tag '${tag:0:40}', not vX.Y.Z"
version=${tag#v}
bundle="clavren-$version-aarch64-linux.tar.gz"

jq --arg checked_at "$now" --arg version "$version" --arg bundle "$bundle" \
    --argjson mounted "$backup_drive_mounted" '
    def asset($name): [(.assets // [])[] | select(.name == $name) | .browser_download_url | strings] | first;
    def https: if type == "string" and startswith("https://") then . else null end;
    (asset($bundle) | https) as $bundle_url
    | (asset($bundle + ".b3") | https) as $checksum_url
    | {
        checked_at: $checked_at,
        version: $version,
        release_url: (.html_url | https),
        published_at: (.published_at | if type == "string" then . else null end),
        installable: ($bundle_url != null and $checksum_url != null),
        bundle_url: $bundle_url,
        checksum_url: $checksum_url,
        backup_drive_mounted: $mounted,
        error: null
    }' "$work/release.json" >"$work/latest.json" 2>/dev/null || failed "the answer from $host is not a release"
publish <"$work/latest.json"

if [[ $(jq -r .installable "$work/latest.json") == true ]]; then
    say "the latest release is $version"
else
    say "the latest release is $version, not installable yet (its bundle isn't uploaded)"
fi
