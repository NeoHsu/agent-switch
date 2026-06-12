use std::{fs, path::Path};

use anyhow::Result;
use serde_json::json;

use crate::{
    config::{self, Config, CONFIG_FILE, LEGACY_CONFIG_FILE},
    fs::{abs, repo_path},
    manifest,
    sync::{self, SyncOptions},
    CommandOutput, ExitCode,
};

pub fn doctor(root: &Path, cfg: Option<&Config>, json_output: bool) -> Result<CommandOutput> {
    let mut out = CommandOutput::default();
    let agents_exists = root.join(".agents").exists();
    let config_exists = root.join(CONFIG_FILE).exists() || root.join(LEGACY_CONFIG_FILE).exists();
    let manifest_ok = root
        .join(".agents/.sync-manifest.json")
        .exists()
        .then(|| manifest::load(&root.join(".agents/.sync-manifest.json")).is_ok())
        .unwrap_or(true);

    if json_output {
        out.push(serde_json::to_string_pretty(&json!({
            "root": repo_path(root),
            "agents_dir": agents_exists,
            "config": config_exists,
            "manifest": manifest_ok,
        }))?);
        return Ok(out);
    }

    out.push(format!("ok       root: {}", root.display()));
    if agents_exists {
        out.push("ok       .agents exists");
    } else {
        out.push("warning: .agents does not exist");
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
        for (link, target) in &cfg.symlinks {
            let link_abs = abs(root, Path::new(link));
            if link_abs.is_symlink() || link_abs.exists() {
                out.push(format!("ok       {} -> {}", link, target));
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
    let _ = fs::metadata(".");
    Ok(out)
}
