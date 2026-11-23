//! Persistence of shows and runtime state.
//!
//! * Show files are JSON with a `schema_version` and explicit migrations.
//! * Writes are atomic: temp file → fsync → rename → fsync of the directory.
//! * Every save rotates the previous N versions into `<name>.backups/`.
//! * Loading never panics; a damaged file falls back to the newest valid backup.

pub mod autosave;
pub mod migrate;

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use klight_model::{RuntimeState, Show};

pub use autosave::{AutosaveConfig, AutosaveStatus, Autosaver};

/// Refuse files larger than this (a show is a few MB at most).
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid JSON: {0}")]
    Json(String),
    #[error("unsupported schema version {found} (this build supports up to {supported})")]
    UnsupportedVersion { found: u64, supported: u32 },
    #[error("migration from version {from} failed: {reason}")]
    Migration { from: u64, reason: String },
    #[error("file too large ({0} bytes)")]
    TooLarge(u64),
    #[error("no valid show found: {0}")]
    NothingValid(String),
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> PersistError + '_ {
    move |source| PersistError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Parses show bytes, migrating older schema versions.
pub fn parse_show_bytes(bytes: &[u8]) -> Result<Show, PersistError> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(PersistError::TooLarge(bytes.len() as u64));
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| PersistError::Json(e.to_string()))?;
    let value = migrate::migrate_to_current(value)?;
    serde_json::from_value(value).map_err(|e| PersistError::Json(e.to_string()))
}

pub fn serialize_show(show: &Show) -> Result<Vec<u8>, PersistError> {
    let mut v = serde_json::to_vec_pretty(show).map_err(|e| PersistError::Json(e.to_string()))?;
    v.push(b'\n');
    Ok(v)
}

fn read_limited(path: &Path) -> Result<Vec<u8>, PersistError> {
    let meta = fs::metadata(path).map_err(io_err(path))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(PersistError::TooLarge(meta.len()));
    }
    fs::read(path).map_err(io_err(path))
}

/// Writes `bytes` to `path` atomically. Never overwrites in place.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), PersistError> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(io_err(dir))?;
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "show".into());
    let tmp = dir.join(format!(".{file_name}.tmp-{}", std::process::id()));
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)
            .map_err(io_err(&tmp))?;
        f.write_all(bytes).map_err(io_err(&tmp))?;
        f.sync_all().map_err(io_err(&tmp))?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(PersistError::Io {
            path: path.to_owned(),
            source: e,
        });
    }
    sync_dir(dir);
    Ok(())
}

/// fsync of the directory so the rename itself is durable (Unix only; on
/// Windows directories cannot be opened this way and NTFS journals renames).
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    {
        match File::open(dir).and_then(|d| d.sync_all()) {
            Ok(()) => {}
            Err(e) => tracing::warn!("fsync of {} failed: {e}", dir.display()),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        let _ = File::open;
    }
}

pub fn backup_dir(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "show".into());
    path.with_file_name(format!("{name}.backups"))
}

fn backup_path(path: &Path, n: usize) -> PathBuf {
    backup_dir(path).join(format!("{n:03}.json"))
}

/// Existing backups, newest first.
pub fn list_backups(path: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for n in 1..=999 {
        let p = backup_path(path, n);
        if p.exists() {
            out.push(p);
        } else {
            break;
        }
    }
    out
}

/// Rotates backups: 001 is the newest. Keeps at most `keep`.
fn rotate_backups(path: &Path, keep: usize) -> Result<(), PersistError> {
    if keep == 0 || !path.exists() {
        return Ok(());
    }
    let dir = backup_dir(path);
    fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    let oldest = backup_path(path, keep);
    if oldest.exists() {
        fs::remove_file(&oldest).map_err(io_err(&oldest))?;
    }
    for n in (1..keep).rev() {
        let from = backup_path(path, n);
        if from.exists() {
            let to = backup_path(path, n + 1);
            fs::rename(&from, &to).map_err(io_err(&from))?;
        }
    }
    // Copy (not move) the current file: it stays valid until the new one is renamed over it.
    let bytes = read_limited(path)?;
    write_atomic(&backup_path(path, 1), &bytes)?;
    Ok(())
}

/// Saves a show atomically, keeping `keep_backups` previous versions.
pub fn save_show(path: &Path, show: &Show, keep_backups: usize) -> Result<(), PersistError> {
    let bytes = serialize_show(show)?;
    if let Err(e) = rotate_backups(path, keep_backups) {
        // A backup problem must not prevent saving the show itself.
        tracing::warn!("backup rotation failed: {e}");
    }
    write_atomic(path, &bytes)
}

#[derive(Debug)]
pub struct LoadedShow {
    pub show: Show,
    /// Set when the main file was damaged and a backup was used instead.
    pub from_backup: Option<PathBuf>,
    pub problems: Vec<String>,
}

/// Loads a show; if the file is damaged, tries backups from newest to oldest.
pub fn load_show(path: &Path) -> Result<LoadedShow, PersistError> {
    let mut problems = Vec::new();
    match read_limited(path).and_then(|b| parse_show_bytes(&b)) {
        Ok(show) => {
            return Ok(LoadedShow {
                show,
                from_backup: None,
                problems,
            })
        }
        Err(e) => problems.push(format!("{}: {e}", path.display())),
    }
    for b in list_backups(path) {
        match read_limited(&b).and_then(|bytes| parse_show_bytes(&bytes)) {
            Ok(show) => {
                tracing::warn!("show file damaged, using backup {}", b.display());
                return Ok(LoadedShow {
                    show,
                    from_backup: Some(b),
                    problems,
                });
            }
            Err(e) => problems.push(format!("{}: {e}", b.display())),
        }
    }
    Err(PersistError::NothingValid(problems.join("; ")))
}

/// Path of the runtime state file next to a show.
pub fn runtime_path(show_path: &Path) -> PathBuf {
    let name = show_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "show".into());
    show_path.with_file_name(format!("{name}.runtime.json"))
}

pub fn save_runtime(path: &Path, state: &RuntimeState) -> Result<(), PersistError> {
    let bytes = serde_json::to_vec(state).map_err(|e| PersistError::Json(e.to_string()))?;
    write_atomic(path, &bytes)
}

pub fn load_runtime(path: &Path) -> Result<RuntimeState, PersistError> {
    let bytes = read_limited(path)?;
    serde_json::from_slice(&bytes).map_err(|e| PersistError::Json(e.to_string()))
}
