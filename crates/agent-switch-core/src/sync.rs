//! Synchronization pipeline for import, export, stale removal, links, and merges.

use std::path::Path;

use anyhow::{Context, Result};

use crate::{
    CommandOutput, Error, ExitCode,
    config::{Config, SyncMode},
    manifest,
    tool::Tool,
};

mod context;
mod event;
mod plan;
mod report;
mod stage;
mod stages;

use crate::fs::{abs, repo_path};
use crate::sync::stages::{ExportStage, ImportStage, MergeStage, RemoveStaleStage, SyncLinksStage};
use context::SyncContext;
use event::SyncEvent;

pub use event::SyncEventKind;
use plan::SyncPlan;
use report::SyncReport;
use stage::SyncStage;

#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    pub check: bool,
    pub import_only: bool,
    pub export_only: bool,
    pub reset_manifest: bool,
    pub json: bool,
    pub event_filter: Option<Vec<event::SyncEventKind>>,
}

pub fn parse_event_filter(values: &[String]) -> Result<Vec<event::SyncEventKind>> {
    Ok(event::parse_event_filter(values)?)
}

pub fn run(
    root: &Path,
    cfg: &Config,
    tools: Option<&[Tool]>,
    mut opts: SyncOptions,
) -> Result<CommandOutput> {
    if opts.import_only && opts.export_only {
        return Err(
            Error::Config("--import-only and --export-only are mutually exclusive".into()).into(),
        );
    }
    if !opts.import_only && !opts.export_only {
        match cfg.sync_mode {
            SyncMode::Full => {}
            SyncMode::CanonicalOnly | SyncMode::ExportOnly => opts.export_only = true,
            SyncMode::ImportOnly => opts.import_only = true,
        }
    }

    let manifest_path = abs(root, &cfg.manifest);
    let manifest_path_display = repo_path(&cfg.manifest);
    let mut manifest = if opts.reset_manifest {
        manifest::Manifest::default()
    } else {
        manifest::load(&manifest_path)
            .with_context(|| format!("failed to read manifest {manifest_path_display}"))?
    };
    let plan = SyncPlan::build(root, cfg, tools)?;
    let ctx = SyncContext::new(root, cfg, tools, opts.check);

    let mut report = SyncReport::default();
    let mut changed = opts.reset_manifest;
    if opts.reset_manifest {
        report.push(SyncEvent::Warning {
            message: format!(
                "reset manifest: rebuilding {manifest_path_display} from current files"
            ),
        });
    }

    let stages: [&dyn SyncStage; 5] = [
        &ImportStage,
        &ExportStage,
        &RemoveStaleStage,
        &SyncLinksStage,
        &MergeStage,
    ];

    for stage in stages {
        if stage.should_run(&opts) {
            changed |= stage.run(&ctx, &plan, &mut manifest, &mut report)?;
        }
    }

    let drift_exit = if opts.check && changed {
        Some(ExitCode::Drift)
    } else {
        None
    };

    if opts.check {
        if changed {
            report.push(SyncEvent::Drift);
        } else {
            report.push(SyncEvent::SyncedNoChanges);
        }

        if opts.json {
            let mut out = CommandOutput::default();
            out.push(report.into_json(changed, &opts, drift_exit.unwrap_or(ExitCode::Ok))?);
            out.exit = drift_exit;
            return Ok(out);
        }

        let mut out = report.into_output(opts.event_filter.as_deref());
        out.exit = drift_exit;
        return Ok(out);
    }

    manifest::save(&manifest_path, &mut manifest)?;
    if !changed && report.is_empty() {
        report.push(SyncEvent::SyncedNoChanges);
    }

    if opts.json {
        let mut out = CommandOutput::default();
        out.push(report.into_json(changed, &opts, ExitCode::Ok)?);
        return Ok(out);
    }

    Ok(report.into_output(opts.event_filter.as_deref()))
}
