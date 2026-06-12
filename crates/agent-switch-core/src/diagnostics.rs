use std::path::Path;

use anyhow::Result;
use serde_json::json;

use crate::{
    config::{self, Config, CONFIG_FILE},
    fs::{abs, repo_path},
    manifest,
    sync::{self, SyncOptions},
    CommandOutput, Error, ExitCode,
};

pub fn doctor(root: &Path, cfg: Option<&Config>, json_output: bool) -> Result<CommandOutput> {
    let mut out = CommandOutput::default();
    let agents_dir = cfg
        .map(|c| c.agents_dir.as_path())
        .unwrap_or_else(|| Path::new(".agents"));
    let agents_exists = abs(root, agents_dir).exists();
    let config_exists = root.join(CONFIG_FILE).exists();
    let manifest_path = cfg
        .map(|c| abs(root, &c.manifest))
        .unwrap_or_else(|| root.join(agents_dir).join(".sync-manifest.json"));
    let manifest_ok = if manifest_path.exists() {
        manifest::load(&manifest_path).is_ok()
    } else {
        true
    };

    if json_output {
        out.push(serde_json::to_string_pretty(&json!({
            "root": root.display().to_string(),
            "agents_dir": agents_exists,
            "agents_dir_path": repo_path(agents_dir),
            "config": config_exists,
            "manifest": manifest_ok,
        }))?);
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
        out.push(format!("ok       {CONFIG_FILE} exists"));
    } else {
        out.push(format!("warning: {CONFIG_FILE} does not exist"));
    }
    if manifest_ok {
        out.push("ok       manifest parseable");
    } else {
        out.push("warning: manifest is not parseable");
    }
    if let Some(cfg) = cfg {
        for (link, spec) in &cfg.symlinks {
            let link_abs = abs(root, Path::new(link));
            if link_abs.is_symlink() || link_abs.exists() {
                out.push(format!("ok       {} -> {}", link, repo_path(spec.target())));
            } else {
                out.push(format!("warning: {} is missing", link));
            }
        }
        let check = sync::run(
            root,
            cfg,
            None,
            SyncOptions {
                check: true,
                import_only: false,
                export_only: false,
                json: false,
                event_filter: None,
            },
        )?;
        if check.exit() == ExitCode::Drift {
            out.push("warning: generated file drift detected");
        } else {
            out.push("ok       generated files are in sync");
        }
    }
    Ok(out)
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
