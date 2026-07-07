use anyhow::Result;

use crate::{
    config::{self, ManagedLink},
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
    for (link, spec) in &ctx.cfg.symlinks {
        if !config::symlink_selected(link, spec, ctx.tools) {
            continue;
        }
        changed |= sync_link_copy(
            ctx,
            manifest,
            report,
            &ManagedLink {
                link: link.into(),
                target: spec.target().to_path_buf(),
                target_config: spec.target_config(),
            },
        )?;
    }

    if ctx
        .tools
        .is_none_or(|tools| tools.contains(&crate::tool::Tool::Claude))
    {
        for link in config::claude_instruction_links(ctx.root)? {
            changed |= sync_link_copy(ctx, manifest, report, &link)?;
        }
    }

    Ok(changed)
}

fn sync_link_copy(
    ctx: &SyncContext,
    manifest: &mut crate::manifest::Manifest,
    report: &mut SyncReport,
    managed: &ManagedLink,
) -> Result<bool> {
    let link_rel = managed.link.as_path();
    let target_rel = managed.target.as_path();
    let link_abs = ctx.abs(link_rel);
    let target_abs = ctx.abs(target_rel);

    if link_abs.is_symlink() || !target_abs.is_file() {
        return Ok(false);
    }

    let rel_target = relative_link(&link_abs, &target_abs);
    if is_fake_symlink(&link_abs, &rel_target, &managed.target_config) {
        report.push(SyncEvent::Warning {
            message: format!(
                "{} is an unrestored git symlink placeholder; run `ags setup`",
                repo_path(link_rel)
            ),
        });
        return Ok(false);
    }

    let link_key = repo_path(link_rel);
    let tracked = manifest.links.get(&link_key).cloned();

    if !link_abs.exists() {
        // Recreate copy fallbacks this tool previously created and tracked.
        // Creating brand-new links is `ags setup`'s job.
        if tracked.is_none() {
            return Ok(false);
        }
        let target_bytes = std::fs::read(&target_abs)?;
        if !ctx.check {
            copy_file(&target_abs, &link_abs)?;
            manifest
                .links
                .insert(link_key, manifest::sha256_bytes(&target_bytes));
        }
        report.push(SyncEvent::Copied {
            from: repo_path(target_rel),
            to: repo_path(link_rel),
        });
        return Ok(true);
    }

    if !link_abs.is_file() {
        return Ok(false);
    }

    let link_bytes = std::fs::read(&link_abs)?;
    let target_bytes = std::fs::read(&target_abs)?;
    let current_hash = manifest::sha256_bytes(&link_bytes);

    let Some(tracked) = tracked else {
        // A real file we never managed. Adopt it silently when it already
        // matches the canonical target; otherwise warn and leave both
        // files untouched — overwriting unmanaged user data is forbidden.
        if link_bytes == target_bytes {
            if !ctx.check {
                manifest.links.insert(link_key, current_hash);
            }
        } else {
            report.push(SyncEvent::Warning {
                message: format!(
                    "{} is an unmanaged real file that differs from {}; merge it manually and run `ags setup`",
                    link_key,
                    repo_path(target_rel)
                ),
            });
        }
        return Ok(false);
    };

    if tracked != current_hash {
        if !ctx.check {
            copy_file(&link_abs, &target_abs)?;
            manifest.links.insert(repo_path(link_rel), current_hash);
        }
        report.push(SyncEvent::Copied {
            from: repo_path(link_rel),
            to: repo_path(target_rel),
        });
        return Ok(true);
    } else if link_bytes != target_bytes {
        let target_hash = manifest::sha256_bytes(&target_bytes);
        if !ctx.check {
            copy_file(&target_abs, &link_abs)?;
            manifest.links.insert(repo_path(link_rel), target_hash);
        }
        report.push(SyncEvent::Copied {
            from: repo_path(target_rel),
            to: repo_path(link_rel),
        });
        return Ok(true);
    }

    Ok(false)
}
