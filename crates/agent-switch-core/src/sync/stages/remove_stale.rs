use std::path::PathBuf;

use anyhow::Result;

use crate::sync::SyncOptions;
use crate::sync::stage::SyncStage;
use crate::sync::{context::SyncContext, plan::SyncPlan, report::SyncReport};

use super::super::event::SyncEvent;

#[derive(Debug)]
pub(crate) struct RemoveStaleStage;

impl SyncStage for RemoveStaleStage {
    fn should_run(&self, opts: &SyncOptions) -> bool {
        !opts.import_only
    }

    fn run(
        &self,
        ctx: &SyncContext,
        plan: &SyncPlan,
        manifest: &mut crate::manifest::Manifest,
        report: &mut SyncReport,
    ) -> Result<bool> {
        remove_stale(ctx, plan, manifest, report)
    }
}

fn remove_stale(
    ctx: &SyncContext,
    plan: &SyncPlan,
    manifest: &mut crate::manifest::Manifest,
    report: &mut SyncReport,
) -> Result<bool> {
    let mut changed = false;
    let keys = manifest.generated.keys().cloned().collect::<Vec<_>>();

    for key in keys {
        if plan.job_dests.contains(&key) {
            continue;
        }
        let dest_rel = PathBuf::from(&key);
        if plan.spec_for_dest(&dest_rel).is_none() {
            continue;
        }

        changed = true;
        if !ctx.check {
            let dest_abs = ctx.abs(&dest_rel);
            if dest_abs.exists() {
                std::fs::remove_file(dest_abs)?;
            }
            manifest.generated.remove(&key);
        }

        report.push(SyncEvent::Removed { path: key });
    }

    Ok(changed)
}
