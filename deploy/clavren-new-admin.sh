#!/usr/bin/env bash
# ------------------------------------------------------------------------------
#+ Authors: 	Ran# <ran.hash@proton.me>
#+ Created: 	2026/11/03 15:25:05.202566
#+ Revised: 	2026/11/03 15:25:05.202566
# ------------------------------------------------------------------------------

# Creates a Clavren admin on the terminal's alternate screen, so the username, QR code,
# authenticator key and recovery codes never reach the scrollback (the password is never shown).
# Installed as /usr/local/bin/clavren-new-admin by install.sh; run it from another machine with
#   ssh -t your-pi clavren-new-admin
# It waits for Enter before wiping the screen, also after a failure, so nothing is missed.
set -euo pipefail

[[ -t 0 && -t 1 ]] || {
    printf 'clavren-new-admin: needs a terminal (ssh -t)\n' >&2
    exit 1
}

tput smcup
trap 'tput rmcup' EXIT
clear
read -rp 'Admin username: ' name
if ! sudo runuser -u clavren -- bash -c 'set -a; . /etc/clavren/clavren.env; exec /usr/local/bin/clavren-admin create-admin "$1"' _ "$name"; then
    read -rsp $'\nIt failed (see above). Press Enter to wipe the screen. '
    exit 1
fi
read -rsp 'Codes saved? Press Enter to wipe the screen. '
