// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 13:04:40.463027
//+ Revised: 	2026/11/03 13:14:23.944162
// -----------------------------------------------------------------------------

//! The first admin account, so the dashboard can be signed in to.

use std::io::{self, BufRead, Write};
use std::path::Path;

use crate::process;

/// The first line `clavren-admin list-admins` prints when there are admins: the table header.
const ADMINS_HEADER: &str = "USERNAME";

/// Asks for a username and runs `clavren-admin create-admin` when the database has no admin yet.
pub fn create_first_if_none(root: &Path) -> Result<(), String> {
    if has_admins(&process::cargo_output(root, "clavren-admin", &["list-admins"])?) {
        return Ok(());
    }
    tracing::info!(target: "xtask::admin", "no admin yet: creating the first one");
    let username = choose(&ask(&format!("Admin username [{}]: ", default_username()))?, &default_username());
    process::cargo_run(root, "clavren-admin", &["create-admin", &username])
}

/// Whether `list-admins` output lists any admin.
pub fn has_admins(list_output: &str) -> bool {
    list_output.lines().next().is_some_and(|line| line.starts_with(ADMINS_HEADER))
}

/// The typed username, or the default when nothing was typed.
pub fn choose(typed: &str, default: &str) -> String {
    // a piped answer from Windows PowerShell starts with a byte-order mark
    let typed = typed.trim_start_matches('\u{feff}').trim();
    if typed.is_empty() { default.to_string() } else { typed.to_string() }
}

fn default_username() -> String {
    let name = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
    choose(&name.to_lowercase(), "admin")
}

fn ask(question: &str) -> Result<String, String> {
    let mut stdout = io::stdout();
    write!(stdout, "{question}").and_then(|()| stdout.flush()).map_err(|error| error.to_string())?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer).map_err(|error| format!("could not read the username: {error}"))?;
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::{choose, has_admins};

    #[test]
    fn the_table_means_there_are_admins() {
        let output = "USERNAME                 STATUS    CREATED           LAST LOGIN\nran   active   2026-11-03 10:00   never\n";
        assert!(has_admins(output));
    }

    #[test]
    fn the_hint_means_there_are_none() {
        assert!(!has_admins("No admins yet. Create one with: clavren-admin create-admin <username>\n"));
        assert!(!has_admins(""));
    }

    #[test]
    fn an_empty_answer_takes_the_default() {
        assert_eq!(choose("  \r\n", "ran"), "ran");
        assert_eq!(choose(" alice\n", "ran"), "alice");
        assert_eq!(choose("\u{feff}\r\n", "ran"), "ran");
    }
}
