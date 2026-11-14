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
#   xtask (the whole crate, left out with --exclude)
#       it drives other programs on the real console: PowerShell, npm, cargo and the interactive
#       create-admin; its decisions (dashboard staleness, admin listing, the local URL) have unit
#       tests, its task dispatch has binary tests, and `cargo dev` is run by hand after changes.
#   crates\clavren-server\src\startup.rs, axum::serve returning an error and the master-key read
#   task being cancelled
#       neither has a known trigger: serve only fails if the listening socket breaks, and the
#       blocking task is never aborted.
#   crates\clavren-server\src\admin_auth\account.rs and passwords.rs, the password check that
#   couldn't run
#       the same blocking task: it only fails when it panics or is aborted, and neither happens (a
#       damaged stored hash is a mismatch, which is tested).
#   the database failing in the middle of a request (applications\submit.rs and customers\mod.rs
#   passing on an error that isn't the unique violation they name, updates\install.rs when the
#   commit or the rename after it fails)
#       a test can close the pool before a request (tested: a plain server error), not between two
#       statements of one.
#   crates\clavren-server\src\crypto\master_key.rs, creating the key on a file system without
#   hard links, and the key file vanishing between the link failing and the read
#       needs a FAT or network volume, or a second process deleting the file in that instant.
#   crates\clavren-server\src\dashboard.rs, the "dashboard not built" pages
#       shown by a binary built without the dashboard; the tests always embed the built one.
#   crates\clavren-server\src\activations\tokens.rs, one field of a debug log line
#       only evaluated at debug level; the tests log from info.
#   crates\clavren-server\src\updates\files.rs, a file whose metadata can't be read
#       needs a file that exists and can't be stat-ed: no portable way to make one.
#   crates\clavren-client\src\lib.rs Client::for_windows
#       reads and locks a file under the real %APPDATA%: a test would write in the profile of
#       whoever runs it. Its parts (FileStore, Dpapi, HttpTransport) are tested by themselves.
#   crates\clavren-client\src\dpapi.rs, Windows refusing to protect, or answering with nothing
#       DPAPI doesn't fail for a signed-in user; the failure when unprotecting is tested.
#   crates\clavren-client\src\store.rs, a temporary file that can't be removed after a failed write
#       needs the write to fail and then the delete of the same file to fail differently.
#   crates\clavren-client\src\online.rs and applying.rs, "changed while asking"
#       another thread activating, or setting the application aside, while this one waits for the
#       server's answer: the fake server answers at once, so there is no such moment to hit.
#   crates\clavren-client\src\background.rs, waiting out a backoff and the thread having panicked
#       the shortest backoff is a minute of real time; a panicking callback is caught before it
#       can end the thread (tested).
param()
$ErrorActionPreference = 'Stop'
$MinLines = 97

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
& "$PSScriptRoot\local-db.ps1" | Out-Null

# cargo reports progress on stderr, which Windows PowerShell would otherwise treat as a failure;
# the exit code below is what decides
$ErrorActionPreference = 'Continue'
cargo llvm-cov --workspace --exclude xtask --show-missing-lines --fail-under-lines $MinLines
if ($LASTEXITCODE -ne 0) { throw "coverage is below $MinLines% of lines, or a test failed" }
