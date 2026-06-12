use anyhow::Result;

use crate::{
    config,
    fs::{copy_file, is_fake_symlink, relative_link, repo_path},
    manifest,
};

use crate::sync::SyncOptions;
use crate::sync::stage::SyncStage;
use crate::sync::{context::SyncContext, plan::SyncPlan, report::SyncReport};

use super::super::event::SyncEvent;

#[derive(Debug)]
pub(crate) struct SyncLinksStage;

impl SyncStage for SyncLinksStage {
    fn should_run(&self, opts: &SyncOptions) -> bool {
        !opts.import_only
    }

    fn run(
        &self,
        ctx: &SyncContext,
        _plan: &SyncPlan,
        manifest: &mut crate::manifest::Manifest,
        report: &mut SyncReport,
    ) -> Result<bool> {
        sync_link_copies(ctx, manifest, report)
    }
}

fn sync_link_copies(
    ctx: &SyncContext,
    manifest: &mut crate::manifest::Manifest,
    report: &mut SyncReport,
) -> Result<bool> {
    let mut changed = false;
    for (link, target) in &ctx.cfg.symlinks {
        if !config::symlink_selected(link, target, ctx.tools) {
            continue;
        }
        let link_rel = std::path::Path::new(link);
        let target_rel = std::path::Path::new(target);
        let link_abs = ctx.abs(link_rel);
        let target_abs = ctx.abs(target_rel);

        if link_abs.is_symlink() || !target_abs.is_file() {
            continue;
        }

        let rel_target = relative_link(&link_abs, &target_abs);
        if is_fake_symlink(&link_abs, &rel_target, target) {
            report.push(SyncEvent::Warning {
                message: format!(
                    "{} is an unrestored git symlink placeholder; run `ags setup`",
                    repo_path(link_rel)
                ),
            });
            continue;
        }

        if !link_abs.exists() && target_abs.exists() {
            changed = true;
            let target_bytes = std::fs::read(&target_abs)?;
            if !ctx.check {
                copy_file(&target_abs, &link_abs)?;
                manifest
                    .links
                    .insert(repo_path(link_rel), manifest::sha256_bytes(&target_bytes));
            }
            report.push(SyncEvent::Copied {
                from: repo_path(target_rel),
                to: repo_path(link_rel),
            });
            continue;
        }

        if !link_abs.is_file() {
            continue;
        }

        let link_bytes = std::fs::read(&link_abs)?;
        let target_bytes = std::fs::read(&target_abs)?;
        let current_hash = manifest::sha256_bytes(&link_bytes);
        let tracked = manifest.links.get(&repo_path(link_rel)).cloned();

        if tracked.as_deref() != Some(&current_hash) {
            changed = true;
            if !ctx.check {
                copy_file(&link_abs, &target_abs)?;
                manifest.links.insert(repo_path(link_rel), current_hash);
            }
            report.push(SyncEvent::Copied {
                from: repo_path(link_rel),
                to: repo_path(target_rel),
            });
        } else if link_bytes != target_bytes {
            changed = true;
            let target_hash = manifest::sha256_bytes(&target_bytes);
            if !ctx.check {
                copy_file(&target_abs, &link_abs)?;
                manifest.links.insert(repo_path(link_rel), target_hash);
            }
            report.push(SyncEvent::Copied {
                from: repo_path(target_rel),
                to: repo_path(link_rel),
            });
        }
    }

    Ok(changed)
}
