use std::{
    fs,
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

pub fn write_if_changed(path: &Path, content: &str) -> Result<bool> {
    if path.exists() && fs::read_to_string(path).unwrap_or_default() == content {
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
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    let trimmed = text.trim();
    trimmed == target_cfg
        || trimmed == repo_path(target_rel)
        || trimmed == target_rel.to_string_lossy()
}

pub fn remove_file_or_empty_dir(path: &Path) -> Result<()> {
    if path.is_dir() && !path.is_symlink() {
        fs::remove_dir(path)?;
    } else if path.exists() || path.is_symlink() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn copy_file(src: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dest)?;
    Ok(())
}
