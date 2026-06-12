use anyhow::Result;

use super::{SyncOptions, context::SyncContext, plan::SyncPlan, report::SyncReport};
use crate::manifest;

pub(super) trait SyncStage {
    fn should_run(&self, opts: &SyncOptions) -> bool;

    fn run(
        &self,
        ctx: &SyncContext,
        plan: &SyncPlan,
        manifest: &mut manifest::Manifest,
        report: &mut SyncReport,
    ) -> Result<bool>;
}
