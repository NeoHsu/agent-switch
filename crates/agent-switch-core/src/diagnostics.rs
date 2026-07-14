//! Doctor and validation commands for repository health checks.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use serde_json::json;

use crate::{
    CommandOutput, Error, ExitCode,
    config::{self, CONFIG_FILE, Config},
    fs::{abs, is_fake_symlink, relative_link, repo_path},
    manifest::{self, Manifest},
    setup,
    sync::{self, SyncOptions},
};

#[derive(Debug, Serialize)]
struct LinkDiagnostic {
    path: String,
    target: String,
    status: String,
    healthy: bool,
}

pub fn doctor(root: &Path, cfg: Option<&Config>, json_output: bool) -> Result<CommandOutput> {
    doctor_at(root, cfg, &root.join(CONFIG_FILE), json_output)
}

pub fn doctor_at(
    root: &Path,
    cfg: Option<&Config>,
    config_path: &Path,
    json_output: bool,
) -> Result<CommandOutput> {
    let mut out = CommandOutput::default();
    let agents_dir = cfg
        .map(|c| c.agents_dir.as_path())
        .unwrap_or_else(|| Path::new(".agents"));
    let agents_exists = abs(root, agents_dir).exists();
    let config_exists = config_path.exists();
    let config_path_display = display_path(root, config_path);
    let manifest_path = cfg
        .map(|c| abs(root, &c.manifest))
        .unwrap_or_else(|| root.join(agents_dir).join(".sync-manifest.json"));
    let manifest_exists = manifest_path.exists();
    let (loaded_manifest, manifest_error) = if manifest_exists {
        match manifest::load(&manifest_path) {
            Ok(manifest) => (Some(manifest), None),
            Err(err) => (None, Some(err.to_string())),
        }
    } else {
        (None, None)
    };
    let manifest_ok = manifest_exists && manifest_error.is_none();
    let manifest_path_display = display_path(root, &manifest_path);
    let manifest_recovery = if !manifest_exists {
        Some("Run `ags sync` to create it.")
    } else if manifest_error.is_some() {
        Some("Run `ags sync --reset-manifest` to rebuild it.")
    } else {
        None
    };

    let mut drift = !agents_exists || !config_exists || !manifest_ok;
    let mut links = Vec::new();
    if let Some(cfg) = cfg {
        for (link, spec) in &cfg.symlinks {
            let diagnostic = diagnose_link(
                root,
                link,
                spec.target(),
                &spec.target_config(),
                loaded_manifest.as_ref(),
            )?;
            drift |= !diagnostic.healthy;
            links.push(diagnostic);
        }
    }

    let generated_files_in_sync = if let Some(cfg) = cfg {
        if manifest_error.is_none() {
            let check = sync::run(
                root,
                cfg,
                None,
                SyncOptions {
                    check: true,
                    export_only: true,
                    ..SyncOptions::default()
                },
            )?;
            let in_sync = check.exit() != ExitCode::Drift;
            drift |= !in_sync;
            Some(in_sync)
        } else {
            None
        }
    } else {
        None
    };

    if json_output {
        out.push(serde_json::to_string_pretty(&json!({
            "root": root.display().to_string(),
            "agents_dir": agents_exists,
            "agents_dir_path": repo_path(agents_dir),
            "config": config_exists,
            "config_path": config_path_display,
            "manifest": manifest_ok,
            "manifest_exists": manifest_exists,
            "manifest_parseable": manifest_exists.then_some(manifest_error.is_none()),
            "manifest_path": manifest_path_display,
            "manifest_error": manifest_error.as_deref(),
            "manifest_recovery": manifest_recovery,
            "links": links,
            "generated_files_in_sync": generated_files_in_sync,
            "drift": drift,
        }))?);
        if drift {
            out.exit = Some(ExitCode::Drift);
        }
        return Ok(out);
    }

    out.push(format!("ok       root: {}", root.display()));
    let agents_dir_display = repo_path(agents_dir);
    if agents_exists {
        out.push(format!("ok       {agents_dir_display} exists"));
    } else {
        out.push(format!("warning: {agents_dir_display} does not exist"));
    }
    if config_exists {
        out.push(format!("ok       {config_path_display} exists"));
    } else {
        out.push(format!("warning: {config_path_display} does not exist"));
    }
    if !manifest_exists {
        out.push(format!(
            "warning: manifest is missing: {manifest_path_display}"
        ));
        out.push("hint:    run `ags sync` to create it");
    } else if let Some(err) = manifest_error.as_deref() {
        out.push(format!(
            "warning: manifest is not parseable: {manifest_path_display}"
        ));
        out.push(format!("error:   {err}"));
        out.push("hint:    run `ags sync --reset-manifest` to rebuild it");
    } else {
        out.push("ok       manifest parseable");
    }

    for link in &links {
        push_link_diagnostic(&mut out, link);
    }
    match generated_files_in_sync {
        Some(true) => out.push("ok       generated files are in sync"),
        Some(false) => out.push("warning: generated file drift detected"),
        None if cfg.is_some() => {
            out.push("warning: generated file drift check skipped until manifest is repaired")
        }
        None => {}
    }

    if drift {
        out.exit = Some(ExitCode::Drift);
    }
    Ok(out)
}

fn diagnose_link(
    root: &Path,
    link: &str,
    target: &Path,
    target_config: &str,
    sync_manifest: Option<&Manifest>,
) -> Result<LinkDiagnostic> {
    let link_rel = Path::new(link);
    let link_abs = abs(root, link_rel);
    let target_abs = abs(root, target);
    let target_display = repo_path(target);

    let (status, healthy) = if !target_abs.exists() {
        ("target_missing", false)
    } else if setup::is_correct_link(&link_abs, &target_abs)? {
        ("ok", true)
    } else {
        let relative_target = relative_link(&link_abs, &target_abs);
        if is_fake_symlink(&link_abs, &relative_target, target_config) {
            ("placeholder", false)
        } else if link_abs.is_symlink() {
            ("wrong_target", false)
        } else if link_abs.is_file()
            && sync_manifest.is_some_and(|manifest| manifest.links.contains_key(link))
        {
            let link_bytes = std::fs::read(&link_abs)?;
            let target_bytes = std::fs::read(&target_abs)?;
            let current_hash = manifest::sha256_bytes(&link_bytes);
            let tracked_hash = sync_manifest.and_then(|manifest| manifest.links.get(link));
            if tracked_hash != Some(&current_hash) {
                ("managed_copy_modified", false)
            } else if link_bytes != target_bytes {
                ("managed_copy_out_of_sync", false)
            } else {
                ("managed_copy", true)
            }
        } else if link_abs.exists() {
            ("unmanaged", false)
        } else {
            ("missing", false)
        }
    };

    Ok(LinkDiagnostic {
        path: repo_path(link_rel),
        target: target_display,
        status: status.to_string(),
        healthy,
    })
}

fn push_link_diagnostic(out: &mut CommandOutput, link: &LinkDiagnostic) {
    match link.status.as_str() {
        "ok" => out.push(format!("ok       {} -> {}", link.path, link.target)),
        "managed_copy" => out.push(format!(
            "ok       {} -> {} (managed copy)",
            link.path, link.target
        )),
        "target_missing" => out.push(format!(
            "warning: {} target is missing: {}",
            link.path, link.target
        )),
        "placeholder" => out.push(format!(
            "warning: {} is an unrestored git symlink placeholder; run `ags setup`",
            link.path
        )),
        "wrong_target" => out.push(format!(
            "warning: {} points to the wrong target; expected {}",
            link.path, link.target
        )),
        "managed_copy_modified" => out.push(format!(
            "warning: {} managed copy was modified; run `ags sync`",
            link.path
        )),
        "managed_copy_out_of_sync" => out.push(format!(
            "warning: {} managed copy differs from {}; run `ags sync`",
            link.path, link.target
        )),
        "unmanaged" => out.push(format!(
            "warning: {} is an unmanaged real file or directory; reconcile it with {} and run `ags setup`",
            link.path, link.target
        )),
        "missing" => out.push(format!("warning: {} is missing", link.path)),
        _ => out.push(format!("warning: {} has unknown status", link.path)),
    }
}

pub fn doctor_config_error(
    root: &Path,
    config_path: &Path,
    err: &anyhow::Error,
    json_output: bool,
) -> Result<CommandOutput> {
    let mut out = CommandOutput {
        exit: Some(exit_for_config_error(err)),
        ..CommandOutput::default()
    };
    let config_path = display_path(root, config_path);
    let error = err.to_string();

    if json_output {
        out.push(serde_json::to_string_pretty(&json!({
            "root": root.display().to_string(),
            "config": false,
            "config_path": config_path,
            "config_error": error,
        }))?);
    } else {
        out.push(format!("ok       root: {}", root.display()));
        out.push(format!("error:   {config_path}: {error}"));
    }

    Ok(out)
}

fn exit_for_config_error(err: &anyhow::Error) -> ExitCode {
    match err.downcast_ref::<Error>() {
        Some(Error::Unsupported(_)) => ExitCode::Unsupported,
        Some(Error::Config(_)) | None => ExitCode::Config,
    }
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(repo_path)
        .unwrap_or_else(|_| path.display().to_string())
}

pub fn validate_mappings(cfg: &Config, json_output: bool) -> Result<CommandOutput> {
    config::validate_config(cfg)?;
    let mut out = CommandOutput::default();
    if json_output {
        out.push(serde_json::to_string_pretty(&json!({
            "valid": true,
            "version": cfg.version,
            "symlinks": cfg.symlinks.len(),
            "generate": cfg.generate.len(),
            "merge": cfg.merge.len(),
        }))?);
    } else {
        out.push("ok       mappings valid");
    }
    Ok(out)
}
