#!/usr/bin/env bash
# ------------------------------------------------------------------------------
#+ Authors: 	Ran# <ran.hash@proton.me>
#+ Created: 	2026/11/09 08:54:54.059029
#+ Revised: 	2026/11/09 08:54:54.059029
# ------------------------------------------------------------------------------

# Prepares a Debian or Ubuntu CI machine for scripts/ci.sh: Node.js 22 for the dashboard and the
# PostgreSQL 17 client programs (pg_dump, pg_restore, psql) for the backup tests. Rust comes from
# rust-toolchain.toml through rustup, which the CI image provides. Runs as root or with sudo.
set -euo pipefail

run() { if ((EUID == 0)); then "$@"; else sudo "$@"; fi; }

run apt-get update
run apt-get install -y --no-install-recommends ca-certificates curl gnupg postgresql-common
# PostgreSQL's own repository: the distributions' clients are older than the 17 the server runs
run /usr/share/postgresql-common/pgdg/apt.postgresql.org.sh -y
if ! node --version 2>/dev/null | grep -q '^v2[2-9]'; then
    curl -fsSL https://deb.nodesource.com/setup_22.x | run bash -
    run apt-get install -y --no-install-recommends nodejs
fi
run apt-get install -y --no-install-recommends postgresql-client-17
node --version
pg_dump --version
