//! Setup command implementation for native tool links and copy fallbacks.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result};

use crate::{
    CommandOutput, ExitCode,
    config::{self, Config, GenerateSpec, ManagedLink},
    fs::{
        abs, io_error, is_fake_symlink, read_text, relative_link, remove_file_or_empty_dir,
        repo_path,
    },
    manifest::{self, Manifest},
    mcp, sync,
    tool::{MergeFormat, Tool},
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
    let manifest_path = abs(root, &cfg.manifest);
    let mut sync_manifest = manifest::load(&manifest_path)
        .with_context(|| format!("failed to read manifest {}", repo_path(&cfg.manifest)))?;
    let mut manifest_changed = false;
    if opts.prune {
        drift |= prune_unselected(
            root,
            cfg,
            tools,
            opts.check,
            &mut sync_manifest,
            &mut manifest_changed,
            &mut out,
        )?;
    }
    for (link, spec) in &cfg.symlinks {
        if !config::symlink_selected(link, spec, tools) {
            continue;
        }
        drift |= setup_link(
            root,
            &ManagedLink {
                link: PathBuf::from(link),
                target: spec.target().to_path_buf(),
                target_config: spec.target_config(),
            },
            opts,
            &mut sync_manifest,
            &mut manifest_changed,
            &mut out,
        )?;
    }

    if tool_selected(Tool::Claude, tools) {
        for link in config::claude_instruction_links(root)? {
            drift |= setup_link(
                root,
                &link,
                opts,
                &mut sync_manifest,
                &mut manifest_changed,
                &mut out,
            )?;
        }
    }

    if manifest_changed && !opts.check {
        manifest::save(&manifest_path, &mut sync_manifest)?;
    }

    if !opts.no_sync {
        let sync_out = sync::run(
            root,
            cfg,
            tools,
            sync::SyncOptions {
                check: opts.check,
                ..sync::SyncOptions::default()
            },
        )?;
        let sync_exit = sync_out.exit();
        if sync_exit == ExitCode::Drift {
            drift = true;
        }
        out.lines.extend(sync_out.lines);
        if sync_exit != ExitCode::Ok {
            out.exit = Some(sync_exit);
        }
    }

    if opts.check && drift {
        out.exit = Some(ExitCode::Drift);
    }
    Ok(out)
}

fn setup_link(
    root: &Path,
    managed: &ManagedLink,
    opts: SetupOptions,
    sync_manifest: &mut Manifest,
    manifest_changed: &mut bool,
    out: &mut CommandOutput,
) -> Result<bool> {
    let link_rel = managed.link.as_path();
    let target_rel_cfg = managed.target.as_path();
    let link_abs = abs(root, link_rel);
    let target_abs = abs(root, target_rel_cfg);
    let rel_target = relative_link(&link_abs, &target_abs);
    let rel_target_display = repo_path(&rel_target);
    let link_key = repo_path(link_rel);

    if !target_abs.exists() {
        out.push(format!(
            "skipped  {}: canonical target is missing: {}",
            repo_path(link_rel),
            repo_path(target_rel_cfg)
        ));
        out.exit = Some(ExitCode::Drift);
        return Ok(true);
    }

    if is_correct_link(&link_abs, &target_abs)? {
        out.push(format!(
            "ok       {} -> {}",
            repo_path(link_rel),
            rel_target_display
        ));
        return Ok(false);
    }

    if is_fake_symlink(&link_abs, &rel_target, &managed.target_config) {
        if opts.check {
            out.push(format!("repaired {}", repo_path(link_rel)));
            return Ok(true);
        }
        remove_file_or_empty_dir(&link_abs)?;
        let is_symlink = create_link_or_fallback(&link_abs, &target_abs, &rel_target)?;
        if !is_symlink {
            record_copy_fallback(sync_manifest, manifest_changed, &link_key, &link_abs)?;
        }
        out.push(link_message(
            "repaired",
            link_rel,
            &rel_target_display,
            is_symlink,
        ));
        return Ok(true);
    }

    if !link_abs.is_symlink() && link_abs.is_file() && sync_manifest.links.contains_key(&link_key) {
        // A copy fallback this tool created earlier; sync reconciles its
        // content, so setup only reports it.
        out.push(format!("ok       {link_key} (managed copy)"));
        return Ok(false);
    }

    if !link_abs.is_symlink()
        && link_abs.is_file()
        && target_abs.is_file()
        && fs::read(&link_abs)? == fs::read(&target_abs)?
    {
        // A pre-existing file with canonical-identical content can be safely
        // adopted as a managed copy without replacing it or risking data loss.
        if !opts.check {
            record_copy_fallback(sync_manifest, manifest_changed, &link_key, &link_abs)?;
        }
        out.push(format!("adopted  {link_key} (managed copy)"));
        return Ok(true);
    }

    if link_abs.exists() || link_abs.is_symlink() {
        if opts.force && link_abs.is_symlink() {
            if !opts.check {
                remove_file_or_empty_dir(&link_abs)?;
                let is_symlink = create_link_or_fallback(&link_abs, &target_abs, &rel_target)?;
                if !is_symlink {
                    record_copy_fallback(sync_manifest, manifest_changed, &link_key, &link_abs)?;
                }
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
            return Ok(true);
        }
        out.push(format!(
            "skipped  {}: existing real file or directory; merge it into {} and remove it before retrying",
            repo_path(link_rel),
            repo_path(target_rel_cfg)
        ));
        out.exit = Some(ExitCode::Drift);
        return Ok(true);
    }

    if !opts.check {
        let is_symlink = create_link_or_fallback(&link_abs, &target_abs, &rel_target)?;
        if !is_symlink {
            record_copy_fallback(sync_manifest, manifest_changed, &link_key, &link_abs)?;
        }
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
    Ok(true)
}

fn prune_unselected(
    root: &Path,
    cfg: &Config,
    tools: Option<&[Tool]>,
    check: bool,
    sync_manifest: &mut Manifest,
    manifest_changed: &mut bool,
    out: &mut CommandOutput,
) -> Result<bool> {
    let Some(tools) = tools else {
        return Ok(false);
    };
    let mut changed = false;

    for (link, spec) in &cfg.symlinks {
        if config::symlink_selected(link, spec, Some(tools)) {
            continue;
        }
        let (did_change, did_manifest_change) = prune_link(
            root,
            sync_manifest,
            &ManagedLink {
                link: PathBuf::from(link),
                target: spec.target().to_path_buf(),
                target_config: spec.target_config(),
            },
            check,
            out,
        )?;
        changed |= did_change;
        *manifest_changed |= did_manifest_change;
    }

    let nested_claude_links = config::claude_instruction_links(root)?;
    if !tool_selected(Tool::Claude, Some(tools)) {
        for link in &nested_claude_links {
            let (did_change, did_manifest_change) =
                prune_link(root, sync_manifest, link, check, out)?;
            changed |= did_change;
            *manifest_changed |= did_manifest_change;
        }
    }

    let mut configured_links = cfg.symlinks.keys().cloned().collect::<BTreeSet<_>>();
    configured_links.extend(nested_claude_links.iter().map(|link| repo_path(&link.link)));
    let stale_links = sync_manifest.links.clone();
    for (link, tracked_hash) in stale_links {
        if configured_links.contains(&link) {
            continue;
        }
        let (did_change, did_manifest_change) =
            prune_stale_copy_fallback(root, sync_manifest, &link, &tracked_hash, check, out)?;
        changed |= did_change;
        *manifest_changed |= did_manifest_change;
    }

    changed |= prune_unselected_generated(
        root,
        cfg,
        tools,
        check,
        sync_manifest,
        manifest_changed,
        out,
    )?;
    changed |= prune_unselected_merges(root, cfg, tools, check, out)?;

    Ok(changed || *manifest_changed)
}

fn prune_unselected_generated(
    root: &Path,
    cfg: &Config,
    tools: &[Tool],
    check: bool,
    sync_manifest: &mut Manifest,
    manifest_changed: &mut bool,
    out: &mut CommandOutput,
) -> Result<bool> {
    let unselected: Vec<GenerateSpec> = cfg
        .generate
        .values()
        .filter(|spec| !config::generate_selected(spec, Some(tools)))
        .cloned()
        .collect();
    if unselected.is_empty() {
        return Ok(false);
    }

    // Outputs the unselected specs would generate from the current canonical
    // sources, plus manifest-tracked outputs whose sources are gone.
    let mut candidates: BTreeMap<String, Option<sync::PlannedOutput>> = BTreeMap::new();
    for job in sync::planned_outputs(root, &unselected)? {
        candidates.insert(repo_path(&job.dest_rel), Some(job));
    }
    for key in sync_manifest.generated.keys() {
        if candidates.contains_key(key) {
            continue;
        }
        let dest_rel = PathBuf::from(key);
        let owner = cfg
            .generate
            .values()
            .filter(|spec| dest_rel.starts_with(&spec.to))
            .max_by_key(|spec| spec.to.components().count());
        if owner.is_some_and(|spec| !config::generate_selected(spec, Some(tools))) {
            candidates.insert(key.clone(), None);
        }
    }

    let mut changed = false;
    for (key, job) in &candidates {
        let dest_abs = abs(root, Path::new(key));
        if !dest_abs.exists() {
            if sync_manifest.generated.remove(key).is_some() {
                *manifest_changed = true;
                changed = true;
                out.push(format!("removed: {key}"));
            }
            continue;
        }
        if !is_managed_generated(root, &dest_abs, key, job.as_ref(), sync_manifest) {
            out.push(format!(
                "skipped  {key}: not recognized as an unmodified generated file; remove it manually"
            ));
            out.exit = Some(ExitCode::Drift);
            continue;
        }
        changed = true;
        if !check {
            fs::remove_file(&dest_abs)
                .map_err(|err| io_error("remove generated file", &dest_abs, err))?;
        }
        if sync_manifest.generated.remove(key).is_some() {
            *manifest_changed = true;
        }
        out.push(format!("removed: {key}"));
    }

    if !check {
        for spec in &unselected {
            let to_abs = abs(root, &spec.to);
            remove_empty_dirs(&to_abs);
            remove_empty_parent(root, &to_abs);
        }
    }
    Ok(changed)
}

/// A generated output may be deleted only when it provably came from this
/// tool: its content matches the manifest hash, or re-exporting its canonical
/// source reproduces it byte for byte.
fn is_managed_generated(
    root: &Path,
    dest_abs: &Path,
    dest_key: &str,
    job: Option<&sync::PlannedOutput>,
    sync_manifest: &Manifest,
) -> bool {
    let Ok(current) = read_text(dest_abs) else {
        return false;
    };
    if sync_manifest
        .generated
        .get(dest_key)
        .is_some_and(|entry| entry.hash == manifest::sha256_text(&current))
    {
        return true;
    }
    let Some(job) = job else {
        return false;
    };
    let Ok(source) = read_text(&abs(root, &job.src_rel)) else {
        return false;
    };
    job.format
        .export(&source)
        .is_ok_and(|generated| generated == current)
}

fn prune_unselected_merges(
    root: &Path,
    cfg: &Config,
    tools: &[Tool],
    check: bool,
    out: &mut CommandOutput,
) -> Result<bool> {
    let canonical_mcp = mcp::canonical_mcp_path(root, &cfg.agents_dir);
    let mut changed = false;
    for (id, spec) in &cfg.merge {
        if config::merge_selected(id, spec, Some(tools)) {
            continue;
        }
        let target = abs(root, &spec.to);
        match mcp::prune(spec.format, &canonical_mcp, &target, check)? {
            mcp::PruneOutcome::Removed => {
                changed = true;
                if !check {
                    remove_empty_parent(root, &target);
                }
                out.push(format!("removed: {}", repo_path(&spec.to)));
            }
            mcp::PruneOutcome::Cleaned => {
                changed = true;
                out.push(format!(
                    "cleaned  {}: removed agent-switch managed MCP servers",
                    repo_path(&spec.to)
                ));
            }
            mcp::PruneOutcome::Unmanaged => {
                out.push(format!(
                    "skipped  {}: not recognized as agent-switch output; remove it manually",
                    repo_path(&spec.to)
                ));
                out.exit = Some(ExitCode::Drift);
            }
            mcp::PruneOutcome::Absent => {}
        }
    }

    let legacy_copilot = Path::new(".copilot/mcp-config.json");
    let legacy_is_configured = cfg
        .merge
        .values()
        .any(|spec| repo_path(&spec.to) == repo_path(legacy_copilot));
    if !legacy_is_configured {
        let target = abs(root, legacy_copilot);
        match mcp::prune(MergeFormat::Copilot, &canonical_mcp, &target, check)? {
            mcp::PruneOutcome::Removed => {
                changed = true;
                if !check {
                    remove_empty_parent(root, &target);
                }
                out.push(format!("removed: {}", repo_path(legacy_copilot)));
            }
            mcp::PruneOutcome::Cleaned => {
                changed = true;
                out.push(format!(
                    "cleaned  {}: removed agent-switch managed MCP servers",
                    repo_path(legacy_copilot)
                ));
            }
            mcp::PruneOutcome::Unmanaged => {
                out.push(format!(
                    "skipped  {}: not recognized as agent-switch output; remove it manually",
                    repo_path(legacy_copilot)
                ));
                out.exit = Some(ExitCode::Drift);
            }
            mcp::PruneOutcome::Absent => {}
        }
    }
    Ok(changed)
}

fn record_copy_fallback(
    sync_manifest: &mut Manifest,
    manifest_changed: &mut bool,
    link_key: &str,
    link_abs: &Path,
) -> Result<()> {
    let bytes = fs::read(link_abs).map_err(|err| io_error("read managed copy", link_abs, err))?;
    sync_manifest
        .links
        .insert(link_key.to_string(), manifest::sha256_bytes(&bytes));
    *manifest_changed = true;
    Ok(())
}

/// Best-effort removal of a now-empty tool directory; `remove_dir` refuses
/// non-empty directories, so user content is never at risk.
fn remove_empty_parent(root: &Path, path: &Path) {
    if let Some(parent) = path.parent() {
        if parent != root {
            let _ = fs::remove_dir(parent);
        }
    }
}

fn remove_empty_dirs(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            remove_empty_dirs(&entry.path());
        }
    }
    let _ = fs::remove_dir(dir);
}

fn prune_stale_copy_fallback(
    root: &Path,
    sync_manifest: &mut manifest::Manifest,
    link_key: &str,
    tracked_hash: &str,
    check: bool,
    out: &mut CommandOutput,
) -> Result<(bool, bool)> {
    let link_rel = Path::new(link_key);
    let safe_path = !link_key.is_empty()
        && !link_key.contains('\\')
        && !link_rel.is_absolute()
        && link_rel
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if !safe_path {
        out.push(format!(
            "skipped  {link_key}: unsafe path in manifest; run `ags sync --reset-manifest`"
        ));
        out.exit = Some(ExitCode::Drift);
        return Ok((false, false));
    }
    let link_abs = abs(root, link_rel);
    if !link_abs.exists() && !link_abs.is_symlink() {
        let manifest_changed = sync_manifest.links.remove(link_key).is_some();
        out.push(format!("removed: {link_key}"));
        return Ok((true, manifest_changed));
    }
    if link_abs.is_file() && !link_abs.is_symlink() {
        let current_hash = manifest::sha256_bytes(&fs::read(&link_abs)?);
        if current_hash == tracked_hash {
            if !check {
                remove_file_or_empty_dir(&link_abs)?;
                remove_empty_parent(root, &link_abs);
            }
            let manifest_changed = sync_manifest.links.remove(link_key).is_some();
            out.push(format!("removed: {link_key}"));
            return Ok((true, manifest_changed));
        }
        out.push(format!(
            "skipped  {link_key}: managed copy was modified; preserve or merge it manually"
        ));
    } else {
        out.push(format!(
            "skipped  {link_key}: stale manifest entry is not a managed file copy; remove it manually"
        ));
    }
    out.exit = Some(ExitCode::Drift);
    Ok((false, false))
}

fn prune_link(
    root: &Path,
    sync_manifest: &mut manifest::Manifest,
    managed: &ManagedLink,
    check: bool,
    out: &mut CommandOutput,
) -> Result<(bool, bool)> {
    let link_rel = managed.link.as_path();
    let target_rel = managed.target.as_path();
    let link_abs = abs(root, link_rel);
    let target_abs = abs(root, target_rel);
    let rel_target = relative_link(&link_abs, &target_abs);
    let link_key = repo_path(link_rel);
    let tracked_hash = sync_manifest.links.get(&link_key).cloned();
    let managed_copy_matches = if let Some(tracked_hash) = &tracked_hash {
        link_abs.is_file()
            && !link_abs.is_symlink()
            && manifest::sha256_bytes(&fs::read(&link_abs)?) == *tracked_hash
    } else {
        false
    };

    let is_managed = is_correct_link(&link_abs, &target_abs)?
        || is_fake_symlink(&link_abs, &rel_target, &managed.target_config)
        || managed_copy_matches;
    let mut changed = false;
    let manifest_changed;
    if is_managed {
        changed = true;
        if !check {
            remove_file_or_empty_dir(&link_abs)?;
            remove_empty_parent(root, &link_abs);
        }
        out.push(format!("removed: {}", link_key));
        manifest_changed = sync_manifest.links.remove(&link_key).is_some();
    } else if tracked_hash.is_some() && !link_abs.exists() {
        changed = true;
        out.push(format!("removed: {}", link_key));
        manifest_changed = sync_manifest.links.remove(&link_key).is_some();
    } else {
        if tracked_hash.is_some() && link_abs.is_file() && !link_abs.is_symlink() {
            out.push(format!(
                "skipped  {link_key}: managed copy was modified; preserve or merge it manually"
            ));
        } else if link_abs.exists() || link_abs.is_symlink() {
            out.push(format!(
                "skipped  {}: existing real file or directory; remove it manually if it is no longer needed",
                link_key
            ));
        }
        if link_abs.exists() || link_abs.is_symlink() {
            out.exit = Some(ExitCode::Drift);
        }
        manifest_changed = false;
    }

    Ok((changed, manifest_changed))
}

fn tool_selected(tool: Tool, tools: Option<&[Tool]>) -> bool {
    tools.is_none_or(|tools| tools.contains(&tool))
}

pub(crate) fn is_correct_link(link: &Path, target: &Path) -> Result<bool> {
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
    use std::process::Command;

    create_link_or_fallback_windows(
        link,
        target,
        rel_target,
        create_dir_symlink,
        create_file_symlink,
        |link, target| {
            let status = Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .status()
                .map_err(|err| io_error("create directory junction", link, err))?;
            Ok(status.success())
        },
    )
}

#[cfg(windows)]
fn create_dir_symlink(rel_target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(rel_target, link)
}

#[cfg(windows)]
fn create_file_symlink(rel_target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(rel_target, link)
}

#[cfg(windows)]
fn create_link_or_fallback_windows<DirSymlink, FileSymlink, Junction>(
    link: &Path,
    target: &Path,
    rel_target: &Path,
    symlink_dir: DirSymlink,
    symlink_file: FileSymlink,
    create_junction: Junction,
) -> Result<bool>
where
    DirSymlink: FnOnce(&Path, &Path) -> std::io::Result<()>,
    FileSymlink: FnOnce(&Path, &Path) -> std::io::Result<()>,
    Junction: FnOnce(&Path, &Path) -> Result<bool>,
{
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| io_error("create parent directory", parent, err))?;
    }
    if target.is_dir() {
        if symlink_dir(rel_target, link).is_ok() {
            return Ok(true);
        }
        if !create_junction(link, target)? {
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

#[cfg(all(test, windows))]
mod windows_tests {
    use std::{fs, io, path::Path};

    use anyhow::Result;
    use tempfile::tempdir;

    use super::create_link_or_fallback_windows;

    #[test]
    fn file_symlink_failure_falls_back_to_copy() -> Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let target = root.join("target.txt");
        let link = root.join("link.txt");
        fs::write(&target, "content\n")?;

        let created_symlink = create_link_or_fallback_windows(
            &link,
            &target,
            Path::new("target.txt"),
            |_, _| unreachable!("directory symlink should not be used for files"),
            |_, _| Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied")),
            |_, _| unreachable!("junction should not be used for files"),
        )?;

        assert!(!created_symlink);
        assert_eq!(fs::read_to_string(link)?, "content\n");
        Ok(())
    }

    #[test]
    fn directory_symlink_failure_uses_junction() -> Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let target = root.join("target");
        let link = root.join("link");
        fs::create_dir(&target)?;

        let created_symlink = create_link_or_fallback_windows(
            &link,
            &target,
            Path::new("target"),
            |_, _| Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied")),
            |_, _| unreachable!("file symlink should not be used for directories"),
            |link, _| {
                fs::create_dir(link)?;
                Ok(true)
            },
        )?;

        assert!(created_symlink);
        assert!(link.exists());
        Ok(())
    }
}
