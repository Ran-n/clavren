// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 13:04:41.708807
//+ Revised: 	2026/11/09 22:30:57.849527
// -----------------------------------------------------------------------------

//! Running the tools the launcher drives: the server's binaries through cargo, npm, PowerShell.

use std::path::Path;
use std::process::{Command, Output};

use crate::interrupt::ChildRunning;

/// Builds the server's binaries, showing cargo's progress.
pub fn cargo_build(root: &Path) -> Result<(), String> {
    let mut command = Command::new(cargo());
    command.current_dir(root).args(["build", "-p", "clavren-server", "--bins"]);
    run(command, "cargo build -p clavren-server")
}

/// Builds and runs one of the server's binaries, attached to this terminal.
pub fn cargo_run(root: &Path, binary: &str, args: &[&str]) -> Result<(), String> {
    run(cargo_command(root, binary, args), &format!("{binary} {}", args.join(" ")))
}

/// Builds and runs one of the server's binaries and returns what it printed.
pub fn cargo_output(root: &Path, binary: &str, args: &[&str]) -> Result<String, String> {
    let mut command = cargo_command(root, binary, args);
    command.stdin(std::process::Stdio::null());
    let running = ChildRunning::start();
    let output = start(command.output(), binary)?;
    drop(running);
    check(&output, binary)?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Runs a command attached to this terminal and fails unless it succeeds.
pub fn run(mut command: Command, what: &str) -> Result<(), String> {
    let _running = ChildRunning::start();
    let status = start(command.status(), what)?;
    if status.success() { Ok(()) } else { Err(format!("`{}` failed ({status})", what.trim())) }
}

/// A command for a program that is a `.cmd` script on Windows (npm).
pub fn script_command(program: &str) -> Command {
    if cfg!(windows) { Command::new(format!("{program}.cmd")) } else { Command::new(program) }
}

fn cargo_command(root: &Path, binary: &str, args: &[&str]) -> Command {
    let mut command = Command::new(cargo());
    command.current_dir(root).args(["run", "--quiet", "-p", "clavren-server", "--bin", binary, "--"]).args(args);
    command
}

/// The cargo that started this launcher, with the same toolchain and `.cargo/config.toml`.
fn cargo() -> std::ffi::OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

fn start<T>(result: std::io::Result<T>, what: &str) -> Result<T, String> {
    result.map_err(|error| format!("could not start `{}`: {error}", what.trim()))
}

fn check(output: &Output, what: &str) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!("`{what}` failed ({}): {}", output.status, stderr.trim()))
}
