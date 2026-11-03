// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 13:23:35.325190
//+ Revised: 	2026/11/03 13:23:35.325190
// -----------------------------------------------------------------------------

//! The `xtask` binary as `cargo xtask` runs it: only known tasks run.

use std::process::Command;

#[test]
fn an_unknown_task_fails_and_names_the_tasks() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask")).arg("nope").output().unwrap();
    assert!(!output.status.success());
    let printed = String::from_utf8_lossy(&output.stdout);
    assert!(printed.contains(r#"unknown task "nope"; the tasks are: dev"#), "{printed}");
}

#[test]
fn no_task_fails_too() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask")).output().unwrap();
    assert!(!output.status.success());
}
