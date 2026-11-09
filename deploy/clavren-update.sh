#!/usr/bin/env bash
# ------------------------------------------------------------------------------
#+ Authors: 	Ran# <ran.hash@proton.me>
#+ Created: 	2026/11/09 09:57:46.001976
#+ Revised: 	2026/11/09 11:10:33.592132
# ------------------------------------------------------------------------------

# Installs a newer Clavren release when the dashboard asks for one. Runs as root, started by
# clavren-update.path when the server writes CLAVREN_DATA_DIR/updates/request.json.
#
# The request comes from the unprivileged server, so it is untrusted: it may only name a version
# (three plain numbers) and whether to skip the backup (true or false), and it is checked before
# anything uses it. Everything else is decided here: the version must be the newest release the
# forge reports right now and newer than the installed one; the bundle comes from that release,
# must match its BLAKE3 checksum, and its own deploy/install.sh does the upgrade (backup first,
# old binaries kept as *.previous, restart), exactly as an upgrade by hand.
#
# Progress goes to CLAVREN_UPDATE_STATUS (default /var/lib/clavren-update/status.json, in a folder
# only root writes; the server reads it), and every line to the journal: journalctl -u clavren-update.
# Needs: curl, jq, b3sum, tar, flock.
set -Eeuo pipefail
umask 022
cd /

readonly DEFAULT_SOURCE=https://api.github.com/repos/Ran-n/clavren/releases/latest
readonly BIN_DIR=/usr/local/bin
# The server reads the same variable (CLAVREN_UPDATE_STATUS); its folder is root's alone.
readonly STATUS_FILE=${CLAVREN_UPDATE_STATUS:-/var/lib/clavren-update/status.json}
readonly STATUS_DIR=${STATUS_FILE%/*}
readonly LOCK=$STATUS_DIR/update.lock
readonly MIN_FREE_KB=$((200 * 1024))
readonly WORK_PARENT=/var/tmp

# The download folder; removed on exit, however the script ends.
work=''
trap '[[ -z $work ]] || rm -rf -- "$work"' EXIT

say() { printf 'clavren-update: %s\n' "$*"; }

# Bash reads a script as it runs, and install.sh replaces this file: everything is in main, which
# is read whole before it starts.
main() {
    local data_dir=${CLAVREN_DATA_DIR:-/var/lib/clavren}
    local source_url=${CLAVREN_UPDATE_SOURCE:-$DEFAULT_SOURCE}
    local updates_dir=$data_dir/updates
    # Only https: the bundle and its checksum are taken from this answer.
    local fetch=(curl -fsS --proto =https --proto-redir =https)
    local version='' stage=starting started_at
    started_at=$(now)
    # Any failure not handled below still ends as "failed", never as an update stuck "running".
    trap 'unexpected $LINENO' ERR

    # One update at a time. systemd never runs the service twice at once; the lock covers a run
    # started by hand.
    install -d -m 0755 "$STATUS_DIR"
    exec 9>"$LOCK"
    if ! flock -n 9; then
        say "another update is running; this request is dropped"
        rm -f -- "$updates_dir/request.json"
        exit 1
    fi

    # The folder belongs to the clavren user: refuse a link, then work inside it by relative names.
    [[ -d $updates_dir && ! -L $updates_dir ]] || refuse "$updates_dir is not a folder."
    cd -P -- "$updates_dir"
    [[ $(pwd -P) == "$updates_dir" ]] || refuse "$updates_dir is not where it should be."
    if [[ ! -e request.json ]]; then
        say "there is no update request; nothing to do"
        exit 0
    fi
    [[ -f request.json && ! -L request.json ]] || refuse "The update request is not a plain file."
    # Taken: the path unit doesn't fire again, and the server sees no pending request.
    mv -f -- request.json request.taken

    local request
    request=$(head -c 1024 request.taken)
    rm -f -- request.taken
    local requested skip
    requested=$(jq -r 'if type == "object" and (keys - ["skip_backup", "version"]) == [] and (.version | type) == "string" then .version else "" end' <<<"$request" 2>/dev/null) ||
        refuse "The update request is not valid."
    [[ $requested =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || refuse "The update request names no valid version."
    version=$requested
    skip=$(jq -r 'if (.skip_backup | type) == "boolean" then .skip_backup else "" end' <<<"$request")
    [[ $skip == true || $skip == false ]] || refuse "The update request doesn't say whether to back up."
    status running "$stage"
    say "updating to $version$([[ $skip == true ]] && printf ' without a backup')"

    [[ $(uname -m) == aarch64 ]] || refuse "Releases are built for 64-bit Raspberry Pi OS (aarch64); this machine is $(uname -m)."
    local installed
    installed=$("$BIN_DIR/clavren-admin" --version 2>/dev/null | awk '{ print $2 }') || installed=''
    [[ -n $installed ]] || refuse "The installed version can't be read ($BIN_DIR/clavren-admin --version failed)."
    if [[ $installed == "$version" || $(printf '%s\n%s\n' "$installed" "$version" | sort -V | tail -n 1) != "$version" ]]; then
        refuse "$version is not newer than the installed $installed."
    fi
    local free_kb
    free_kb=$(df -Pk "$WORK_PARENT" | awk 'NR == 2 { print $4 }')
    ((free_kb >= MIN_FREE_KB)) || refuse "Less than 200 MB is free in $WORK_PARENT; free some space and try again."

    stage=downloading
    status running "$stage"
    work=$(mktemp -d "$WORK_PARENT/clavren-update.XXXXXX")
    # Ask the forge again rather than trust anything the server wrote.
    [[ $source_url =~ ^https://[^[:space:]]+$ ]] || refuse "CLAVREN_UPDATE_SOURCE must be an https:// address."
    "${fetch[@]}" --max-time 30 -H 'Accept: application/vnd.github+json' -H 'User-Agent: clavren-update' \
        -o "$work/release.json" "$source_url" || refuse "The release list could not be fetched; try again later."
    local bundle="clavren-$version-aarch64-linux.tar.gz" urls
    urls=$(jq -r --arg tag "v$version" --arg bundle "$bundle" '
        def asset($name): [(.assets // [])[] | select(.name == $name) | .browser_download_url | strings
                           | select(startswith("https://"))] | first // "";
        if .tag_name != $tag then "" else "\(asset($bundle)) \(asset($bundle + ".b3"))" end' "$work/release.json" 2>/dev/null) ||
        refuse "The forge's answer is not a release; nothing was installed."
    local bundle_url checksum_url
    read -r bundle_url checksum_url <<<"$urls"
    [[ -n $bundle_url ]] || refuse "$version is no longer the newest release; reload the page to see the current one."
    [[ -n $checksum_url ]] || refuse "The $version release has no checksum file yet; try again once it is uploaded."
    "${fetch[@]}" -L --max-time 300 -o "$work/$bundle" "$bundle_url" || refuse "The download of $version failed; nothing was installed."
    "${fetch[@]}" -L --max-time 60 -o "$work/$bundle.b3" "$checksum_url" || refuse "The checksum download failed; nothing was installed."

    stage=checking
    status running "$stage"
    local hash name
    read -r hash name <"$work/$bundle.b3" || hash=''
    [[ $hash =~ ^[0-9a-f]{64}$ && $name == "$bundle" ]] || refuse "The checksum file is not for $bundle, so nothing was installed."
    (cd "$work" && b3sum --check --quiet "$bundle.b3" >/dev/null 2>&1) ||
        refuse "The download didn't match its checksum, so nothing was installed."
    tar -xzf "$work/$bundle" -C "$work" || refuse "The bundle could not be unpacked; nothing was installed."
    local installer=$work/clavren-$version-aarch64-linux/deploy/install.sh
    [[ -f $installer ]] || refuse "The bundle has no deploy/install.sh; nothing was installed."

    stage=$([[ $skip == true ]] && printf installing || printf backing_up)
    status running "$stage"
    local args=() result
    [[ $skip == true ]] && args+=(--skip-backup)
    trap - ERR
    set +e
    bash "$installer" "${args[@]}" </dev/null 2>&1 | follow
    result=${PIPESTATUS[0]}
    set -e
    trap 'unexpected $LINENO' ERR
    # How far install.sh got, by the lines it printed (none: it stopped in its own checks).
    local reached
    reached=$(cat "$work/stage" 2>/dev/null) || reached=''
    stage=${reached:-$stage}
    if ((result != 0)); then
        case $reached in
            backing_up) refuse "The backup before the update failed, so nothing was installed; see journalctl -u clavren-backup." ;;
            installing | restarting)
                refuse "The update stopped after the new files were copied. Go back to the previous version (deployment.md, Going back): sudo cp -p $BIN_DIR/clavren.previous $BIN_DIR/clavren && sudo cp -p $BIN_DIR/clavren-admin.previous $BIN_DIR/clavren-admin && sudo systemctl restart clavren" ;;
            *) refuse "The installer stopped before changing anything; see journalctl -u clavren-update." ;;
        esac
    fi
    status succeeded "$stage" '' "$(now)"
    say "updated to $version"
}

now() { date -u +%Y-%m-%dT%H:%M:%SZ; }

# Writes status.json in one step, in root's own folder.
status() {
    local state=$1 at_stage=$2 message=${3:-} finished=${4:-}
    local temporary
    temporary=$(mktemp "$STATUS_DIR/.status.XXXXXX")
    jq -n --arg version "$version" --arg state "$state" --arg stage "$at_stage" --arg message "$message" \
        --arg started_at "$started_at" --arg finished_at "$finished" '{
            version: (if $version == "" then null else $version end),
            state: $state,
            stage: $stage,
            message: (if $message == "" then null else $message end),
            started_at: $started_at,
            finished_at: (if $finished_at == "" then null else $finished_at end)
        }' >"$temporary"
    chmod 0644 "$temporary"
    mv -f -- "$temporary" "$STATUS_FILE"
}

# Records why the update can't go on, for the dashboard and the journal, and stops.
refuse() {
    trap - ERR
    say "$1" >&2
    status failed "$stage" "$1" "$(now)" || say "the status file could not be written either" >&2
    exit 1
}

unexpected() {
    # In a $(…) the outer command fails too and reports it; one report is enough.
    ((BASH_SUBSHELL == 0)) || exit 1
    refuse "The update stopped unexpectedly at the $stage step (clavren-update.sh line $1); see journalctl -u clavren-update."
}

# Passes install.sh's lines to the journal and follows its stages.
follow() {
    local line
    while IFS= read -r line; do
        printf '%s\n' "$line"
        case $line in
            *"backing up before the upgrade"*) stage=backing_up ;;
            *"installing files"*) stage=installing ;;
            *"starting clavren"*) stage=restarting ;;
            *) continue ;;
        esac
        printf '%s' "$stage" >"$work/stage"
        status running "$stage"
    done
}

main "$@"
exit
