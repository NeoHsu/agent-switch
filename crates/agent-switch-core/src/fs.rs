use std::{
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::Result;

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
        Err(err) => Err(err.into()),
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

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
            Err(err) => return Err(err.into()),
        };

        if let Err(err) = file.write_all(bytes).and_then(|()| file.sync_all()) {
            let _ = fs::remove_file(&temp_path);
            return Err(err.into());
        }
        drop(file);

        if let Err(err) = replace_file(&temp_path, path) {
            let _ = fs::remove_file(&temp_path);
            return Err(err.into());
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
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    let file_type = metadata.file_type();

    if file_type.is_symlink() {
        remove_symlink(path, &file_type)?;
    } else if file_type.is_dir() {
        fs::remove_dir(path)?;
    } else {
        fs::remove_file(path)?;
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
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dest)?;
    Ok(())
}
