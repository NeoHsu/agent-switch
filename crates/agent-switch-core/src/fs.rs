use std::{
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::Result;
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
    if path.exists() && fs::read(path).is_ok_and(|bytes| bytes == content.as_bytes()) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(true)
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
