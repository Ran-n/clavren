# Starts (or stops) the local PostgreSQL cluster used for development and tests, in
# target\local-postgres, listening on 127.0.0.1:55432 with passwordless local access. It never
# touches any other PostgreSQL server on this machine. It holds:
#   clavren_dev   the database `cargo dev` and `cargo run` use (kept between runs)
#   test_*        one database per test, created and dropped by the test itself
# Its data is disposable: fsync is off for speed, so a crash of the machine can lose it.
#   .\scripts\local-db.ps1          create the cluster if needed and start it
#   .\scripts\local-db.ps1 -Stop    stop it
#   .\scripts\local-db.ps1 -Reset   stop it and delete it (and all development data)
# `cargo dev` starts it by itself; `cargo run` and `cargo test` find it through the URLs in .cargo\config.toml.
param([switch]$Stop, [switch]$Reset)
$ErrorActionPreference = 'Stop'

$Port = 55432
$SuperUser = 'clavren_local'
$DevDatabase = 'clavren_dev'
$root = Split-Path -Parent $PSScriptRoot
$cluster = Join-Path $root 'target\local-postgres'
$logFile = Join-Path $root 'target\local-postgres.log'

function Find-PostgresBin {
    $onPath = Get-Command pg_ctl -ErrorAction SilentlyContinue
    if ($onPath) { return Split-Path -Parent $onPath.Source }
    $installed = Get-ChildItem 'C:\Program Files\PostgreSQL\*\bin\pg_ctl.exe' -ErrorAction SilentlyContinue |
        Sort-Object FullName -Descending | Select-Object -First 1
    if ($installed) { return $installed.DirectoryName }
    throw 'PostgreSQL binaries (pg_ctl, initdb) not found: install PostgreSQL 17 or put its bin folder on PATH'
}

$bin = Find-PostgresBin
$pgCtl = Join-Path $bin 'pg_ctl.exe'
$initDb = Join-Path $bin 'initdb.exe'
$psql = Join-Path $bin 'psql.exe'

function Test-Running { & $pgCtl status -D $cluster *> $null; return $LASTEXITCODE -eq 0 }

function Start-Cluster {
    # Many connections: the suite runs its database tests in parallel. pg_ctl gets output pipes of
    # its own: the server process inherits pg_ctl's output handles, and if those were this
    # script's, whoever reads the script's output would wait until the server stops.
    $options = "-p $Port -c listen_addresses=127.0.0.1 -c max_connections=200 -c fsync=off"
    $start = New-Object System.Diagnostics.ProcessStartInfo
    $start.FileName = $pgCtl
    $start.Arguments = "start -D `"$cluster`" -l `"$logFile`" -w -o `"$options`""
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::Start($start)
    # WaitForExit with a timeout doesn't wait for the pipes to close (the server keeps them open)
    if (-not $process.WaitForExit(60000) -or $process.ExitCode -ne 0) {
        throw "the local database did not start; see $logFile"
    }
}

function New-DevDatabase {
    $query = "SELECT 1 FROM pg_database WHERE datname = '$DevDatabase'"
    $exists = & $psql -h 127.0.0.1 -p $Port -U $SuperUser -d postgres -tAc $query
    if ($LASTEXITCODE -ne 0) { throw 'could not query the local database' }
    if ($exists -ne '1') {
        & $psql -h 127.0.0.1 -p $Port -U $SuperUser -d postgres -qc "CREATE DATABASE $DevDatabase" | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "could not create $DevDatabase" }
    }
}

if ($Stop -or $Reset) {
    if ((Test-Path $cluster) -and (Test-Running)) { & $pgCtl stop -D $cluster -m fast | Out-Null }
    if ($Reset -and (Test-Path $cluster)) { Remove-Item -Recurse -Force $cluster }
    Write-Output 'local database stopped'
    exit 0
}

if (-not (Test-Path (Join-Path $cluster 'PG_VERSION'))) {
    & $initDb -D $cluster -U $SuperUser --auth=trust --encoding=UTF8 --no-locale | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'initdb failed' }
}
if (-not (Test-Running)) { Start-Cluster }
New-DevDatabase
Write-Output "local database running on 127.0.0.1:$Port (development database: $DevDatabase)"
# the status probe above leaves pg_ctl's "not running" code behind
exit 0
