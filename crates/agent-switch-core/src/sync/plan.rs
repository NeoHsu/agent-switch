use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use anyhow::Result;
use walkdir::WalkDir;

use crate::{
    Error,
    config::{self, Config, GenerateSpec},
    fs::{abs, repo_path},
    tool::Tool,
};

#[derive(Debug, Clone)]
pub(crate) struct Job {
    pub(crate) format: crate::tool::Format,
    pub(crate) src_rel: PathBuf,
    pub(crate) dest_rel: PathBuf,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct PlanLimits {
    pub(super) max_files: Option<usize>,
    pub(super) max_source_bytes: Option<u64>,
}

#[derive(Debug)]
pub(super) struct SyncPlan {
    pub(super) specs: Vec<GenerateSpec>,
    pub(super) jobs: Vec<Job>,
    pub(super) job_dests: BTreeSet<String>,
}

impl SyncPlan {
    pub(super) fn build(
        root: &Path,
        cfg: &Config,
        tools: Option<&[Tool]>,
        limits: PlanLimits,
    ) -> Result<Self> {
        let specs = selected_specs(cfg, tools);
        let jobs = build_jobs(root, &specs, limits)?;
        let job_dests = jobs
            .iter()
            .map(|job| repo_path(&job.dest_rel))
            .collect::<BTreeSet<_>>();
        Ok(Self {
            specs,
            jobs,
            job_dests,
        })
    }

    pub(super) fn spec_for_dest(&self, dest: &Path) -> Option<&GenerateSpec> {
        self.specs
            .iter()
            .filter(|spec| dest.starts_with(&spec.to))
            .max_by_key(|spec| spec.to.components().count())
    }
}

fn selected_specs(cfg: &Config, tools: Option<&[Tool]>) -> Vec<GenerateSpec> {
    cfg.generate
        .values()
        .filter(|spec| config::generate_selected(spec, tools))
        .cloned()
        .collect()
}

/// Compute the generated outputs the given specs would produce. Shared with
/// `setup --prune`, which needs the output list for unselected specs.
pub(crate) fn planned_outputs(root: &Path, specs: &[GenerateSpec]) -> Result<Vec<Job>> {
    build_jobs(root, specs, PlanLimits::default())
}

fn build_jobs(root: &Path, specs: &[GenerateSpec], limits: PlanLimits) -> Result<Vec<Job>> {
    let mut jobs = Vec::new();
    let mut source_files = 0usize;
    let mut source_bytes = 0u64;
    let mut dest_sources = BTreeMap::<String, PathBuf>::new();

    for spec in specs {
        let from_abs = abs(root, &spec.from);
        if !from_abs.exists() {
            continue;
        }
        let suffix = spec.suffix.clone().unwrap_or_default();
        for entry in WalkDir::new(&from_abs) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let is_markdown = path
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("md"));
            if !is_markdown {
                continue;
            }
            let rel_to_from = path.strip_prefix(&from_abs)?.to_path_buf();
            if !spec.recursive && rel_to_from.components().count() > 1 {
                continue;
            }

            source_files += 1;
            if let Some(max_files) = limits.max_files {
                if source_files > max_files {
                    return Err(Error::Config(format!(
                        "sync generated-file limit exceeded: more than {max_files} Markdown sources would be processed"
                    ))
                    .into());
                }
            }

            source_bytes = source_bytes.saturating_add(entry.metadata()?.len());
            if let Some(max_source_bytes) = limits.max_source_bytes {
                if source_bytes > max_source_bytes {
                    return Err(Error::Config(format!(
                        "sync source-byte limit exceeded: more than {max_source_bytes} bytes would be read"
                    ))
                    .into());
                }
            }

            let rel_no_ext = rel_to_from.with_extension("");
            let file_name = rel_no_ext
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let mut dest_sub = rel_no_ext.clone();
            dest_sub.set_file_name(format!("{file_name}{suffix}"));
            let src_rel = spec.from.join(rel_to_from);
            let dest_rel = spec.to.join(dest_sub);
            let dest_key = repo_path(&dest_rel);
            if let Some(existing) = dest_sources.insert(dest_key.clone(), src_rel.clone()) {
                return Err(Error::Config(format!(
                    "generate output collision: {dest_key} would be produced by both {} and {}",
                    repo_path(&existing),
                    repo_path(&src_rel)
                ))
                .into());
            }
            jobs.push(Job {
                format: spec.format,
                src_rel,
                dest_rel,
            });
        }
    }
    jobs.sort_by(|a, b| a.dest_rel.cmp(&b.dest_rel));
    Ok(jobs)
}
