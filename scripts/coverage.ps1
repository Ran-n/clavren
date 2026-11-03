# Runs the whole test suite under coverage and fails when line coverage drops below the gate,
# listing each file's uncovered lines. Starts the local database first.
#   .\scripts\coverage.ps1
# Needs cargo-llvm-cov (cargo install cargo-llvm-cov); llvm-tools comes with the pinned toolchain.
# Raise MinLines as coverage grows; never lower it.
#
# Lines left uncovered on purpose (everything else the report lists needs a test):
#   crates\clavren-server\src\startup.rs shutdown_signal
#       waits for Ctrl+C or SIGTERM sent to the process; a test can't send either to itself on
#       Windows without stopping the test run. Shutdown itself is tested by passing serve() a
#       future the test completes (the_server_answers_then_shuts_down_cleanly).
#   crates\clavren-server\src\bin\clavren.rs, the successful exit and the runtime failing to start
#       the binary only exits successfully after such a signal; the tests stop it by killing it.
#   crates\clavren-server\src\admin_cli\terminal.rs StdTerminal
#       reads the real console (rpassword hides typing); the tests drive the CLI through a scripted
#       Terminal instead.
#   crates\clavren-server\src\admin_cli\enrolment.rs, the QR code failing to render
#       only happens for data over ~2.9 KB; a setup address is under 200 bytes.
#   crates\clavren-server\src\startup.rs, axum::serve returning an error and the master-key read
#   task being cancelled
#       neither has a known trigger: serve only fails if the listening socket breaks, and the
#       blocking task is never aborted.
param()
$ErrorActionPreference = 'Stop'
$MinLines = 97

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
& "$PSScriptRoot\local-db.ps1" | Out-Null

# cargo reports progress on stderr, which Windows PowerShell would otherwise treat as a failure;
# the exit code below is what decides
$ErrorActionPreference = 'Continue'
cargo llvm-cov --workspace --show-missing-lines --fail-under-lines $MinLines
if ($LASTEXITCODE -ne 0) { throw "coverage is below $MinLines% of lines, or a test failed" }
