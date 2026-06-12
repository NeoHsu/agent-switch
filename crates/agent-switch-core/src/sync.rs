use std::path::Path;

use anyhow::{Context, Result};

use crate::{CommandOutput, Error, ExitCode, config::Config, manifest, tool::Tool};

mod context;
mod event;
mod plan;
mod report;
mod stage;
mod stages;

use crate::fs::abs;
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
    opts: SyncOptions,
) -> Result<CommandOutput> {
    if opts.import_only && opts.export_only {
        return Err(
            Error::Config("--import-only and --export-only are mutually exclusive".into()).into(),
        );
    }

    let manifest_path = abs(root, &cfg.manifest);
    let mut manifest = manifest::load(&manifest_path).context("failed to read manifest")?;
    let plan = SyncPlan::build(root, cfg, tools)?;
    let ctx = SyncContext::new(root, cfg, tools, opts.check);

    let mut report = SyncReport::default();
    let mut changed = false;

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
