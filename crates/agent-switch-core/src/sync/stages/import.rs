use std::path::PathBuf;

use anyhow::Result;
use strum::IntoEnumIterator;

use crate::{
    formats,
    fs::{read_text, repo_path, write_if_changed},
    manifest::{self, GeneratedEntry},
    tool::{Format, Tool},
};

use crate::sync::SyncOptions;
use crate::sync::stage::SyncStage;
use crate::sync::{context::SyncContext, plan::SyncPlan, report::SyncReport};

use super::super::event::SyncEvent;

#[derive(Debug)]
pub(crate) struct ImportStage;

impl SyncStage for ImportStage {
    fn should_run(&self, opts: &SyncOptions) -> bool {
        !opts.export_only
    }

    fn run(
        &self,
        ctx: &SyncContext,
        plan: &SyncPlan,
        manifest: &mut crate::manifest::Manifest,
        report: &mut SyncReport,
    ) -> Result<bool> {
        import_changed(ctx, plan, manifest, report)
    }
}

fn import_changed(
    ctx: &SyncContext,
    plan: &SyncPlan,
    manifest: &mut crate::manifest::Manifest,
    report: &mut SyncReport,
) -> Result<bool> {
    let mut changed = false;
    let dest_keys = manifest.generated.keys().cloned().collect::<Vec<_>>();

    for dest in dest_keys {
        let Some(entry) = manifest.generated.get(&dest).cloned() else {
            continue;
        };
        let dest_rel = PathBuf::from(&dest);
        let Some(spec) = plan.spec_for_dest(&dest_rel) else {
            continue;
        };
        let dest_abs = ctx.abs(&dest_rel);
        if !dest_abs.exists() {
            continue;
        }

        let generated_text = read_text(&dest_abs)?;
        let generated_hash = manifest::sha256_text(&generated_text);
        if generated_hash == entry.hash {
            continue;
        }

        let src_rel = PathBuf::from(&entry.src);
        let src_abs = ctx.abs(&src_rel);
        let existing_src = src_abs.exists().then(|| read_text(&src_abs).ok()).flatten();
        let src_changed = existing_src
            .as_deref()
            .map(|text| manifest::sha256_text(text) != entry.src_hash)
            .unwrap_or(false);

        let mut canonical = spec.format.import(&dest_rel, &generated_text)?;
        if let Some(existing) = &existing_src {
            canonical = preserve_existing_canonical_fields(existing, &canonical, spec.format)?;
        }

        let src_hash = manifest::sha256_text(&canonical);
        changed = true;

        if !ctx.check {
            write_if_changed(&src_abs, &canonical)?;
            manifest.generated.insert(
                dest.clone(),
                GeneratedEntry {
                    hash: generated_hash,
                    src: repo_path(&src_rel),
                    src_hash,
                },
            );
        }

        report.push(SyncEvent::Imported {
            dest,
            src: repo_path(&src_rel),
            conflict: src_changed,
        });
    }

    Ok(changed)
}

fn preserve_existing_canonical_fields(
    existing: &str,
    imported: &str,
    format: Format,
) -> Result<String> {
    let existing_doc = formats::markdown::parse(existing)?;
    let mut imported_doc = formats::markdown::parse(imported)?;
    let current_tool = format.tool().name();
    for (key, value) in existing_doc.frontmatter {
        let key_str = key.as_str();
        let is_other_tool_ns =
            Tool::iter().any(|tool| tool.name() == key_str) && key_str != current_tool;
        let preserve = matches!(key_str, "tools" | "model") || is_other_tool_ns;
        if preserve && !imported_doc.frontmatter.contains_key(key_str) {
            imported_doc.frontmatter.insert(key_str, value);
        }
    }
    formats::markdown::render(imported_doc.frontmatter, &imported_doc.body)
}
