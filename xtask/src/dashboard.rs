// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 13:04:40.772096
//+ Revised: 	2026/11/03 13:14:24.271710
// -----------------------------------------------------------------------------

//! Building the dashboard (`web/admin`) before the server embeds it, only when it is out of date.

use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

use crate::process;

/// Folders under `web/admin` that are not dashboard sources: outputs, dependencies, test files.
const NOT_SOURCES: &[&str] = &["node_modules", "dist", "e2e", "test-results", "playwright-report"];

/// Installs the dependencies when `package-lock.json` changed, and builds into `dist` when any
/// source is newer than the last build.
pub fn build_if_stale(admin: &Path) -> Result<(), String> {
    if is_older(&admin.join("node_modules/.package-lock.json"), &admin.join("package-lock.json"))? {
        tracing::info!(target: "xtask::dashboard", "installing the dashboard's dependencies");
        npm(admin, "ci")?;
    }
    let built = modified(&admin.join("dist/index.html"))?;
    if built.is_none_or(|built| newest_source(admin).is_ok_and(|newest| newest > built)) {
        tracing::info!(target: "xtask::dashboard", "building the dashboard");
        npm(admin, "run build")?;
    } else {
        tracing::info!(target: "xtask::dashboard", "dashboard up to date");
    }
    Ok(())
}

/// Whether `output` is missing or older than `input`.
pub fn is_older(output: &Path, input: &Path) -> Result<bool, String> {
    match (modified(output)?, modified(input)?) {
        (None, _) => Ok(true),
        (Some(output), Some(input)) => Ok(output < input),
        (Some(_), None) => Ok(false),
    }
}

/// The latest modification time of any dashboard source under `folder`.
pub fn newest_source(folder: &Path) -> io::Result<SystemTime> {
    let mut newest = SystemTime::UNIX_EPOCH;
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let time = if entry.file_type()?.is_dir() {
            if NOT_SOURCES.contains(&entry.file_name().to_string_lossy().as_ref()) {
                continue;
            }
            newest_source(&entry.path())?
        } else {
            entry.metadata()?.modified()?
        };
        newest = newest.max(time);
    }
    Ok(newest)
}

fn modified(path: &Path) -> Result<Option<SystemTime>, String> {
    match fs::metadata(path).and_then(|metadata| metadata.modified()) {
        Ok(time) => Ok(Some(time)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("could not read {}: {error}", path.display())),
    }
}

fn npm(admin: &Path, args: &str) -> Result<(), String> {
    let mut command = process::script_command("npm");
    command.current_dir(admin).args(args.split(' '));
    process::run(command, &format!("npm {args}"))
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use super::{is_older, newest_source};

    fn touch(path: &Path, seconds: u64) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = File::create(path).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)).unwrap();
    }

    #[test]
    fn a_missing_output_is_older() {
        let folder = tempfile::tempdir().unwrap();
        touch(&folder.path().join("input"), 10);
        assert!(is_older(&folder.path().join("missing"), &folder.path().join("input")).unwrap());
    }

    #[test]
    fn outputs_compare_by_modification_time() {
        let folder = tempfile::tempdir().unwrap();
        let (input, output) = (folder.path().join("input"), folder.path().join("output"));
        touch(&input, 20);
        touch(&output, 10);
        assert!(is_older(&output, &input).unwrap());
        touch(&output, 30);
        assert!(!is_older(&output, &input).unwrap());
        assert!(!is_older(&output, &folder.path().join("no input")).unwrap());
    }

    #[test]
    fn the_newest_source_ignores_outputs_dependencies_and_tests() {
        let folder = tempfile::tempdir().unwrap();
        touch(&folder.path().join("src/pages/Overview.tsx"), 50);
        touch(&folder.path().join("index.html"), 40);
        for skipped in ["node_modules/x.js", "dist/index.html", "e2e/flows.spec.ts", "test-results/a.png"] {
            touch(&folder.path().join(skipped), 90);
        }
        assert_eq!(newest_source(folder.path()).unwrap(), SystemTime::UNIX_EPOCH + Duration::from_secs(50));
    }
}
