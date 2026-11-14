// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/09 09:57:45.672709
//+ Revised: 	2026/11/14 11:36:30.429684
// -----------------------------------------------------------------------------

//! The files the server shares with the update scripts in `CLAVREN_DATA_DIR/updates`:
//!
//! - `latest.json`, written by `clavren-update-check.sh`: the newest release and how the last
//!   check went.
//! - `status.json`, written by the root `clavren-update.sh` in its own folder (see
//!   [`crate::config::UpdateSettings::status`]): how the current or last update went.
//! - `check-request` and `request.json`, written here: a path unit turns each into a run of its
//!   script. A file is the only channel from the server to them; the server gains no privilege.

use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::version::Version;

pub const LATEST: &str = "latest.json";
pub const CHECK_REQUEST: &str = "check-request";
pub const INSTALL_REQUEST: &str = "request.json";

/// What the last check found. Release fields are kept from the last good check when one fails.
#[derive(Debug, Clone, Deserialize)]
pub struct LatestCheck {
    pub checked_at: DateTime<Utc>,
    pub version: Option<Version>,
    pub release_url: Option<ReleaseUrl>,
    pub published_at: Option<DateTime<Utc>>,
    /// Both bundle files are uploaded.
    #[serde(default)]
    pub installable: bool,
    #[serde(default)]
    pub backup_drive_mounted: bool,
    /// Why the last check failed, in one English sentence.
    pub error: Option<String>,
}

/// A release page: always https://, since the dashboard links to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct ReleaseUrl(String);

impl TryFrom<String> for ReleaseUrl {
    type Error = String;

    fn try_from(url: String) -> Result<Self, Self::Error> {
        let host = url.strip_prefix("https://").unwrap_or_default();
        if host.is_empty() || url.contains(|character: char| character.is_whitespace() || character == '"') {
            return Err(format!("{url:?} is not an https:// address"));
        }
        Ok(Self(url))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateState {
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStage {
    Starting,
    Downloading,
    Checking,
    BackingUp,
    Installing,
    Restarting,
}

/// How the current or last update went, as `clavren-update.sh` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateStatus {
    /// `None` when the request itself was refused before a version could be read.
    pub version: Option<Version>,
    pub state: UpdateState,
    pub stage: Option<UpdateStage>,
    /// For a failure: what went wrong and what to do, in plain English.
    pub message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// What the server asks `clavren-update.sh` to do. Only these two fields: the root script decides
/// everything else itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallRequest {
    pub version: Version,
    pub skip_backup: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("{path} is not valid: {source}")]
    Invalid { path: PathBuf, source: serde_json::Error },
    #[error("cannot write {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
}

/// The file at `path`, parsed; `None` when it doesn't exist.
pub async fn read<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, FileError> {
    let path = path.to_path_buf();
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(FileError::Read { path, source }),
    };
    serde_json::from_slice(&bytes).map(Some).map_err(|source| FileError::Invalid { path, source })
}

/// Writes `name` in `dir` in one step: a reader (a path unit, a script) sees the whole file or
/// none of it.
pub async fn write(dir: &Path, name: &str, contents: &[u8]) -> Result<(), FileError> {
    stage(dir, name, contents).await?.publish().await
}

/// A file written whole under a temporary name, not yet under its own: nothing reading `name`
/// sees it until [`Staged::publish`].
pub struct Staged {
    temporary: PathBuf,
    path: PathBuf,
}

/// Writes `contents` next to `name`, ready to [`Staged::publish`] in one step.
pub async fn stage(dir: &Path, name: &str, contents: &[u8]) -> Result<Staged, FileError> {
    let path = dir.join(name);
    let failed = |source| FileError::Write { path: path.clone(), source };
    tokio::fs::create_dir_all(dir).await.map_err(failed)?;
    let temporary = dir.join(format!("{name}.new"));
    tokio::fs::write(&temporary, contents).await.map_err(failed)?;
    Ok(Staged { temporary, path })
}

impl Staged {
    /// Puts the file under its own name, replacing the previous one in one step.
    pub async fn publish(self) -> Result<(), FileError> {
        let result = tokio::fs::rename(&self.temporary, &self.path).await;
        result.map_err(|source| FileError::Write { path: self.path.clone(), source })
    }

    /// Removes the temporary file, leaving the one under its own name as it was.
    pub async fn discard(self) -> Result<(), FileError> {
        remove(&self.temporary).await
    }
}

/// Removes what [`stage`] left for `name` and never published.
pub async fn discard(dir: &Path, name: &str) -> Result<(), FileError> {
    remove(&dir.join(format!("{name}.new"))).await
}

async fn remove(path: &Path) -> Result<(), FileError> {
    match tokio::fs::remove_file(path).await {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            Err(FileError::Write { path: path.to_path_buf(), source: error })
        }
        _ => Ok(()),
    }
}

/// How long ago the file at `path` was last written, by the system clock; `None` when it doesn't
/// exist (any more). A file dated in the future counts as just written.
pub async fn age(path: &Path) -> Result<Option<std::time::Duration>, FileError> {
    let read_failed = |source| FileError::Read { path: path.to_path_buf(), source };
    let modified = match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata.modified().map_err(read_failed)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(read_failed(source)),
    };
    Ok(Some(std::time::SystemTime::now().duration_since(modified).unwrap_or_default()))
}

#[cfg(test)]
mod tests;
