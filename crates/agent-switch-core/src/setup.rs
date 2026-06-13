use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::Result;

use crate::{
    CommandOutput, ExitCode,
    config::{self, Config},
    fs::{abs, io_error, is_fake_symlink, relative_link, remove_file_or_empty_dir, repo_path},
    manifest, sync,
    tool::Tool,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct SetupOptions {
    pub no_sync: bool,
    pub check: bool,
    pub force: bool,
    pub prune: bool,
}

pub fn run(
    root: &Path,
    cfg: &Config,
    tools: Option<&[Tool]>,
    opts: SetupOptions,
) -> Result<CommandOutput> {
    let mut out = CommandOutput::default();
    let mut drift = false;
    if opts.prune {
        drift |= prune_unselected(root, cfg, tools, opts.check, &mut out)?;
    }
    for (link, spec) in &cfg.symlinks {
        if !config::symlink_selected(link, spec, tools) {
            continue;
        }
        let link_rel = Path::new(link);
        let target_rel_cfg = spec.target();
        let target_cfg = spec.target_config();
        let link_abs = abs(root, link_rel);
        let target_abs = abs(root, target_rel_cfg);
        let rel_target = relative_link(&link_abs, &target_abs);
        let rel_target_display = repo_path(&rel_target);

        if is_correct_link(&link_abs, &target_abs)? {
            out.push(format!(
                "ok       {} -> {}",
                repo_path(link_rel),
                rel_target_display
            ));
            continue;
        }

        if is_fake_symlink(&link_abs, &rel_target, &target_cfg) {
            drift = true;
            if opts.check {
                out.push(format!("repaired {}", repo_path(link_rel)));
                continue;
            }
            remove_file_or_empty_dir(&link_abs)?;
            let is_symlink = create_link_or_fallback(&link_abs, &target_abs, &rel_target)?;
            out.push(link_message(
                "repaired",
                link_rel,
                &rel_target_display,
                is_symlink,
            ));
            continue;
        }

        if link_abs.exists() || link_abs.is_symlink() {
            if opts.check {
                drift = true;
            }
            if opts.force && link_abs.is_symlink() {
                drift = true;
                if !opts.check {
                    remove_file_or_empty_dir(&link_abs)?;
                    let is_symlink = create_link_or_fallback(&link_abs, &target_abs, &rel_target)?;
                    out.push(link_message(
                        "repaired",
                        link_rel,
                        &rel_target_display,
                        is_symlink,
                    ));
                } else {
                    out.push(format!(
                        "repaired {} -> {}",
                        repo_path(link_rel),
                        rel_target_display
                    ));
                }
            } else {
                out.push(format!(
                    "skipped  {}: existing real file or directory; merge it into {} and remove it before retrying",
                    repo_path(link_rel),
                    repo_path(target_rel_cfg)
                ));
            }
            continue;
        }

        drift = true;
        if !opts.check {
            let is_symlink = create_link_or_fallback(&link_abs, &target_abs, &rel_target)?;
            out.push(link_message(
                "created ",
                link_rel,
                &rel_target_display,
                is_symlink,
            ));
        } else {
            out.push(format!(
                "created  {} -> {}",
                repo_path(link_rel),
                rel_target_display
            ));
        }
    }

    if opts.check && drift {
        out.exit = Some(ExitCode::Drift);
        return Ok(out);
    }

    if !opts.no_sync && !opts.check {
        let sync_out = sync::run(root, cfg, tools, sync::SyncOptions::default())?;
        out.lines.extend(sync_out.lines);
        out.exit = sync_out.exit;
    }
    Ok(out)
}

fn prune_unselected(
    root: &Path,
    cfg: &Config,
    tools: Option<&[Tool]>,
    check: bool,
    out: &mut CommandOutput,
) -> Result<bool> {
    let Some(tools) = tools else {
        return Ok(false);
    };
    let manifest_path = abs(root, &cfg.manifest);
    let mut sync_manifest = manifest::load(&manifest_path)?;
    let mut changed = false;
    let mut manifest_changed = false;

    for (link, spec) in &cfg.symlinks {
        if config::symlink_selected(link, spec, Some(tools)) {
            continue;
        }
        let link_rel = Path::new(link);
        let target_rel = spec.target();
        let target_cfg = spec.target_config();
        let link_abs = abs(root, link_rel);
        let target_abs = abs(root, target_rel);
        let rel_target = relative_link(&link_abs, &target_abs);
        let link_key = repo_path(link_rel);
        let had_manifest_link = sync_manifest.links.contains_key(&link_key);

        let managed = is_correct_link(&link_abs, &target_abs)?
            || is_fake_symlink(&link_abs, &rel_target, &target_cfg)
            || (had_manifest_link && link_abs.is_file());
        if managed {
            changed = true;
            if !check {
                remove_file_or_empty_dir(&link_abs)?;
            }
            out.push(format!("removed: {}", link_key));
        } else if had_manifest_link && !link_abs.exists() {
            changed = true;
            out.push(format!("removed: {}", link_key));
        } else if link_abs.exists() || link_abs.is_symlink() {
            out.push(format!(
                "skipped  {}: existing real file or directory; remove it manually if it is no longer needed",
                link_key
            ));
        }

        if sync_manifest.links.remove(&link_key).is_some() {
            manifest_changed = true;
        }
    }

    if manifest_changed && !check {
        manifest::save(&manifest_path, &mut sync_manifest)?;
    }

    Ok(changed || manifest_changed)
}

fn is_correct_link(link: &Path, target: &Path) -> Result<bool> {
    if !link.is_symlink() {
        return Ok(false);
    }
    let dest = fs::read_link(link)?;
    let resolved = if dest.is_absolute() {
        dest
    } else {
        link.parent().unwrap_or_else(|| Path::new("")).join(dest)
    };
    Ok(paths_equivalent(&resolved, target))
}

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => normalize_lexical(left) == normalize_lexical(right),
    }
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Returns `true` if a real symlink (or junction on Windows) was created,
/// `false` if the platform fell back to a plain file copy.
#[cfg(unix)]
fn create_link_or_fallback(link: &Path, _target: &Path, rel_target: &Path) -> Result<bool> {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| io_error("create parent directory", parent, err))?;
    }
    std::os::unix::fs::symlink(rel_target, link)
        .map_err(|err| io_error("create symlink", link, err))?;
    Ok(true)
}

#[cfg(windows)]
fn create_link_or_fallback(link: &Path, target: &Path, rel_target: &Path) -> Result<bool> {
    use std::os::windows::fs::{symlink_dir, symlink_file};
    use std::process::Command;
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| io_error("create parent directory", parent, err))?;
    }
    if target.is_dir() {
        if symlink_dir(rel_target, link).is_ok() {
            return Ok(true);
        }
        let status = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .status()
            .map_err(|err| io_error("create directory junction", link, err))?;
        if !status.success() {
            anyhow::bail!(
                "failed to create directory junction for {}; \
                 enable Developer Mode or run as administrator to allow symlinks",
                link.display()
            );
        }
        Ok(true)
    } else if symlink_file(rel_target, link).is_ok() {
        Ok(true)
    } else {
        // File symlinks require Developer Mode or administrator rights on Windows.
        // Fall back to a plain copy so the tool remains functional.
        crate::fs::copy_file(target, link)?;
        Ok(false)
    }
}

#[cfg(not(any(unix, windows)))]
fn create_link_or_fallback(link: &Path, target: &Path, _rel_target: &Path) -> Result<bool> {
    crate::fs::copy_file(target, link)?;
    Ok(false)
}

fn link_message(prefix: &str, link: &Path, rel_target: &str, is_symlink: bool) -> String {
    if is_symlink {
        format!("{prefix} {} -> {rel_target}", repo_path(link))
    } else {
        format!(
            "{prefix} {} (copy; symlinks unavailable — enable Developer Mode on Windows)",
            repo_path(link)
        )
    }
}
