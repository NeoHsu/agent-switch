use anyhow::Result;

use crate::{config, fs::repo_path, mcp};

use crate::sync::SyncOptions;
use crate::sync::stage::SyncStage;
use crate::sync::{context::SyncContext, report::SyncReport};

use super::super::event::SyncEvent;

#[derive(Debug)]
pub(crate) struct MergeStage;

impl SyncStage for MergeStage {
    fn should_run(&self, opts: &SyncOptions) -> bool {
        !opts.import_only
    }

    fn run(
        &self,
        ctx: &SyncContext,
        _plan: &crate::sync::plan::SyncPlan,
        _manifest: &mut crate::manifest::Manifest,
        report: &mut SyncReport,
    ) -> Result<bool> {
        merge_configs(ctx, report)
    }
}

fn merge_configs(ctx: &SyncContext, report: &mut SyncReport) -> Result<bool> {
    let mut changed = false;
    let canonical_mcp = mcp::canonical_mcp_path(ctx.root, &ctx.cfg.agents_dir);

    for (id, spec) in &ctx.cfg.merge {
        if !config::merge_selected(id, spec, ctx.tools) {
            continue;
        }
        let target = ctx.abs(&spec.to);
        let did_change = mcp::merge(spec.format, &canonical_mcp, &target, ctx.check)?;
        if did_change {
            changed = true;
            report.push(SyncEvent::Merged {
                path: repo_path(&spec.to),
            });
        }
    }

    Ok(changed)
}
