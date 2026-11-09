#!/usr/bin/env bash
# ------------------------------------------------------------------------------
#+ Authors: 	Ran# <ran.hash@proton.me>
#+ Created: 	2026/11/09 08:54:53.971947
#+ Revised: 	2026/11/09 08:54:53.971947
# ------------------------------------------------------------------------------

# The checks every push runs, on GitHub, GitLab and Codeberg alike (see docs/development.md).
# Needs DATABASE_URL pointing at a PostgreSQL 17 server where that user may create databases.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

: "${DATABASE_URL:?DATABASE_URL must point at a PostgreSQL 17 server}"

# the dashboard first: the server embeds web/admin/dist, and its tests check the files
(cd web/admin && npm ci && npm run typecheck && npm run build)
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
