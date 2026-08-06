//! Filesystem helpers for repository-relative paths and atomic writes.

use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use fs2::FileExt;
use serde::{Deserialize, Serialize};

/// Per-repository lock held by mutating CLI commands.
pub const REPOSITORY_LOCK_FILE: &str = ".agent-switch.lock";
/// Journal left behind when a mutating invocation does not complete normally.
pub const REPOSITORY_OPERATION_FILE: &str = ".agent-switch.operation.json";
/// Version of the on-disk interrupted-operation journal.
pub const OPERATION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub struct RepositoryLock {
    file: File,
}

impl RepositoryLock {
    /// Acquire the repository lock, waiting for another `ags` process to finish.
    pub fn acquire(root: &Path) -> Result<Self> {
        fs::create_dir_all(root).map_err(|err| io_error("create repository root", root, err))?;
        let path = root.join(REPOSITORY_LOCK_FILE);
        ensure_regular_state_path(&path, "repository lock")?;

        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|err| io_error("open repository lock", &path, err))?;
        file.lock_exclusive()
            .map_err(|err| io_error("lock repository", &path, err))?;
        Ok(Self { file })
    }
}

impl Drop for RepositoryLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn ensure_regular_state_path(path: &Path, description: &str) -> Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(anyhow::anyhow!(
                "refusing unsafe {description} path: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Durable marker for a mutating invocation that may have been interrupted.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct OperationRecord {
    pub schema_version: u32,
    pub command: String,
    pub pid: u32,
    pub started_at_unix_secs: u64,
}

#[derive(Debug)]
pub struct RepositoryOperation {
    path: PathBuf,
    recovered: Option<OperationRecord>,
}

impl RepositoryOperation {
    /// Start a journaled repository operation after the repository lock is held.
    pub fn begin(root: &Path, command: &str) -> Result<Self> {
        let path = root.join(REPOSITORY_OPERATION_FILE);
        ensure_regular_state_path(&path, "repository operation journal")?;
        let recovered = match fs::symlink_metadata(&path) {
            Ok(_) => {
                let content = read_text(&path)
                    .map_err(|err| io_error("read repository operation journal", &path, err))?;
                let record: OperationRecord = serde_json::from_str(&content).map_err(|err| {
                    anyhow::anyhow!(
                        "repository operation journal is not parseable: {}: {err}",
                        path.display()
                    )
                })?;
                if record.schema_version != OPERATION_SCHEMA_VERSION {
                    return Err(anyhow::anyhow!(
                        "unsupported repository operation journal version {}: {}",
                        record.schema_version,
                        path.display()
                    ));
                }
                Some(record)
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => return Err(io_error("inspect repository operation journal", &path, err)),
        };
        let record = OperationRecord {
            schema_version: OPERATION_SCHEMA_VERSION,
            command: command.to_string(),
            pid: process::id(),
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_secs()),
        };
        let text = format!("{}\n", serde_json::to_string_pretty(&record)?);
        atomic_write(&path, text.as_bytes())?;
        Ok(Self { path, recovered })
    }

    pub fn recovered(&self) -> Option<&OperationRecord> {
        self.recovered.as_ref()
    }

    /// Clear the journal after all command mutations have completed.
    pub fn complete(&self) -> Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(io_error(
                "clear repository operation journal",
                &self.path,
                err,
            )),
        }
    }
}

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);
use pathdiff::diff_paths;

pub fn repo_path(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn abs(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

/// Read a file as UTF-8 text, stripping a leading UTF-8 BOM if present.
/// Windows editors sometimes write BOM-prefixed files that would otherwise
/// cause YAML/TOML/JSON parsers to fail.
pub fn read_text(path: &Path) -> io::Result<String> {
    let s = fs::read_to_string(path)?;
    Ok(if s.starts_with('\u{FEFF}') {
        s['\u{FEFF}'.len_utf8()..].to_string()
    } else {
        s
    })
}

pub fn write_if_changed(path: &Path, content: &str) -> Result<bool> {
    match fs::read(path) {
        Ok(bytes) if bytes == content.as_bytes() => Ok(false),
        Ok(_) => {
            atomic_write(path, content.as_bytes())?;
            Ok(true)
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            atomic_write(path, content.as_bytes())?;
            Ok(true)
        }
        Err(err) => Err(io_error("read existing file", path, err)),
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|err| io_error("create parent directory", parent, err))?;

    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "cannot write to path without a file name: {}",
                path.display()
            ),
        )
    })?;

    let mut last_collision = None;
    for _ in 0..16 {
        let temp_path = next_temp_path(parent, file_name);
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                last_collision = Some(err);
                continue;
            }
            Err(err) => return Err(io_error("create temporary file", &temp_path, err)),
        };

        if let Err(err) = file.write_all(bytes).and_then(|()| file.sync_all()) {
            let _ = fs::remove_file(&temp_path);
            return Err(io_error("write temporary file", &temp_path, err));
        }
        drop(file);

        if let Err(err) = replace_file(&temp_path, path) {
            let _ = fs::remove_file(&temp_path);
            return Err(io_error("replace file", path, err));
        }
        return Ok(());
    }

    Err(last_collision
        .unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "failed to allocate temporary file name",
            )
        })
        .into())
}

fn next_temp_path(parent: &Path, file_name: &OsStr) -> PathBuf {
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut temp_name = OsString::from(".");
    temp_name.push(file_name);
    temp_name.push(format!(".{}.{}.tmp", process::id(), counter));
    parent.join(temp_name)
}

#[cfg(windows)]
fn replace_file(temp_path: &Path, dest: &Path) -> io::Result<()> {
    match fs::rename(temp_path, dest) {
        Ok(()) => Ok(()),
        Err(_) if dest.is_file() || dest.is_symlink() => {
            fs::remove_file(dest)?;
            fs::rename(temp_path, dest)
        }
        Err(err) => Err(err),
    }
}

#[cfg(not(windows))]
fn replace_file(temp_path: &Path, dest: &Path) -> io::Result<()> {
    fs::rename(temp_path, dest)
}

pub fn relative_link(from_link_path: &Path, target: &Path) -> PathBuf {
    let base = from_link_path.parent().unwrap_or_else(|| Path::new(""));
    diff_paths(target, base).unwrap_or_else(|| target.to_path_buf())
}

pub fn is_fake_symlink(path: &Path, target_rel: &Path, target_cfg: &str) -> bool {
    if !path.is_file() {
        return false;
    }
    if fs::metadata(path).is_ok_and(|metadata| metadata.len() > 4096) {
        return false;
    }
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    let trimmed = text.trim();
    // A git-restored symlink placeholder can take three forms depending on
    // who wrote it and which OS normalised the path:
    //   1. the original config string (forward-slash, as in .agent-switch.yaml)
    //   2. the normalised repo path (always forward-slash)
    //   3. the OS-native path (backslashes on Windows)
    trimmed == target_cfg
        || trimmed == repo_path(target_rel)
        || trimmed == target_rel.to_string_lossy()
}

pub fn remove_file_or_empty_dir(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(io_error("inspect path before removal", path, err)),
    };
    let file_type = metadata.file_type();

    if file_type.is_symlink() {
        remove_symlink(path, &file_type).map_err(|err| io_error("remove symlink", path, err))?;
    } else if file_type.is_dir() {
        fs::remove_dir(path).map_err(|err| io_error("remove empty directory", path, err))?;
    } else {
        fs::remove_file(path).map_err(|err| io_error("remove file", path, err))?;
    }
    Ok(())
}

#[cfg(windows)]
fn remove_symlink(path: &Path, file_type: &fs::FileType) -> io::Result<()> {
    use std::os::windows::fs::FileTypeExt;

    if file_type.is_symlink_dir() {
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(not(windows))]
fn remove_symlink(path: &Path, _file_type: &fs::FileType) -> io::Result<()> {
    fs::remove_file(path)
}

pub fn copy_file(src: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| io_error("create parent directory", parent, err))?;
    }
    fs::copy(src, dest)
        .map_err(|err| io_error(&format!("copy {} to", src.display()), dest, err))?;
    Ok(())
}

pub fn io_error(action: &str, path: &Path, err: io::Error) -> anyhow::Error {
    if err.kind() == io::ErrorKind::PermissionDenied {
        anyhow::anyhow!(
            "permission denied while trying to {action} {}: {err}",
            path.display()
        )
    } else {
        err.into()
    }
}
