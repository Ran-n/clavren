// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 13:04:40.772096
//+ Revised: 	2026/11/09 22:41:13.536135
// -----------------------------------------------------------------------------

//! Building the dashboard (`web/admin`) before the server embeds it, only when it is out of date.

use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

use crate::process;

/// Folders under `web/admin` that are not dashboard sources: outputs, dependencies, test files.
const NOT_SOURCES: &[&str] = &["node_modules", "dist", "e2e", "test-results", "playwright-report"];
/// The list of sources the last build was made from (outside `dist`, which the server embeds).
const BUILT_FROM: &str = "node_modules/.clavren-built-from";

/// The dashboard's sources: every file's path (relative, `/`-separated, sorted) and the latest
/// modification time among them.
#[derive(Debug, PartialEq)]
pub struct Sources {
    pub files: Vec<String>,
    pub newest: SystemTime,
}

/// Installs the dependencies when `package-lock.json` changed, and builds into `dist` when any
/// source is newer than the last build, or a source was added, removed or renamed since.
pub fn build_if_stale(admin: &Path) -> Result<(), String> {
    if is_older(&admin.join("node_modules/.package-lock.json"), &admin.join("package-lock.json"))? {
        tracing::info!(target: "xtask::dashboard", "installing the dashboard's dependencies");
        npm(admin, "ci")?;
    }
    let sources = sources(admin).map_err(|error| format!("could not look through the dashboard sources: {error}"))?;
    let list = sources.files.join("\n");
    let built_from = match fs::read_to_string(admin.join(BUILT_FROM)) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("could not read {}: {error}", admin.join(BUILT_FROM).display())),
    };
    let stale = match modified(&admin.join("dist/index.html"))? {
        None => true,
        Some(built) => sources.newest > built || built_from.as_deref() != Some(list.as_str()),
    };
    if stale {
        tracing::info!(target: "xtask::dashboard", "building the dashboard");
        npm(admin, "run build")?;
        fs::write(admin.join(BUILT_FROM), list)
            .map_err(|error| format!("could not write {}: {error}", admin.join(BUILT_FROM).display()))?;
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

/// The dashboard sources under `folder`.
pub fn sources(folder: &Path) -> io::Result<Sources> {
    let mut found = Sources { files: Vec::new(), newest: SystemTime::UNIX_EPOCH };
    collect(folder, "", &mut found)?;
    found.files.sort();
    Ok(found)
}

fn collect(folder: &Path, prefix: &str, found: &mut Sources) -> io::Result<()> {
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_dir() {
            if !NOT_SOURCES.contains(&name.as_str()) {
                collect(&entry.path(), &format!("{prefix}{name}/"), found)?;
            }
        } else {
            found.newest = found.newest.max(entry.metadata()?.modified()?);
            found.files.push(format!("{prefix}{name}"));
        }
    }
    Ok(())
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

    use super::{is_older, sources};

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
    fn the_sources_leave_out_outputs_dependencies_and_tests() {
        let folder = tempfile::tempdir().unwrap();
        touch(&folder.path().join("src/pages/Overview.tsx"), 50);
        touch(&folder.path().join("index.html"), 40);
        for skipped in ["node_modules/x.js", "dist/index.html", "e2e/flows.spec.ts", "test-results/a.png"] {
            touch(&folder.path().join(skipped), 90);
        }
        let found = sources(folder.path()).unwrap();
        assert_eq!(found.files, ["index.html", "src/pages/Overview.tsx"]);
        assert_eq!(found.newest, SystemTime::UNIX_EPOCH + Duration::from_secs(50));
    }

    /// Regression: only modification times counted, so deleting a source left the old build.
    #[test]
    fn a_deleted_source_changes_the_list() {
        let folder = tempfile::tempdir().unwrap();
        touch(&folder.path().join("src/kept.tsx"), 50);
        touch(&folder.path().join("src/removed.tsx"), 50);
        let built = sources(folder.path()).unwrap();
        fs::remove_file(folder.path().join("src/removed.tsx")).unwrap();
        let now = sources(folder.path()).unwrap();
        assert_eq!(now.newest, built.newest, "no newer file");
        assert_ne!(now.files, built.files, "but another list");
    }
}
