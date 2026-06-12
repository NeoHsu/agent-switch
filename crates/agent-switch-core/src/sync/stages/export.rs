use anyhow::{Context, Result};

use crate::{
    fs::{read_text, repo_path, write_if_changed},
    manifest::{self, GeneratedEntry},
};

use crate::sync::SyncOptions;
use crate::sync::stage::SyncStage;
use crate::sync::{context::SyncContext, plan::SyncPlan, report::SyncReport};

use super::super::event::SyncEvent;

#[derive(Debug)]
pub(crate) struct ExportStage;

impl SyncStage for ExportStage {
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
        export_jobs(ctx, plan, manifest, report)
    }
}

fn export_jobs(
    ctx: &SyncContext,
    plan: &SyncPlan,
    manifest: &mut crate::manifest::Manifest,
    report: &mut SyncReport,
) -> Result<bool> {
    let mut changed = false;

    for job in &plan.jobs {
        let src_abs = ctx.abs(&job.src_rel);
        let dest_abs = ctx.abs(&job.dest_rel);
        let source = read_text(&src_abs)?;
        let generated = job
            .format
            .export(&source)
            .with_context(|| format!("failed to export {}", repo_path(&job.src_rel)))?;
        let generated_hash = manifest::sha256_text(&generated);
        let src_hash = manifest::sha256_text(&source);
        let dest_key = repo_path(&job.dest_rel);
        let write_needed = if ctx.check {
            !dest_abs.exists() || read_text(&dest_abs).unwrap_or_default() != generated
        } else {
            write_if_changed(&dest_abs, &generated)?
        };

        if write_needed {
            changed = true;
            report.push(SyncEvent::Generated {
                dest: dest_key.clone(),
            });
        }

        if !ctx.check {
            manifest.generated.insert(
                dest_key,
                GeneratedEntry {
                    hash: generated_hash,
                    src: repo_path(&job.src_rel),
                    src_hash,
                },
            );
        }
    }

    Ok(changed)
}
