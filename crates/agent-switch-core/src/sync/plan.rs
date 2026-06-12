use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use anyhow::Result;
use walkdir::WalkDir;

use crate::{
    config::{self, Config, GenerateSpec},
    fs::{abs, repo_path},
    tool::Tool,
};

#[derive(Debug, Clone)]
pub(super) struct Job {
    pub(super) format: crate::tool::Format,
    pub(super) src_rel: PathBuf,
    pub(super) dest_rel: PathBuf,
}

#[derive(Debug)]
pub(super) struct SyncPlan {
    pub(super) specs: Vec<GenerateSpec>,
    pub(super) jobs: Vec<Job>,
    pub(super) job_dests: BTreeSet<String>,
}

impl SyncPlan {
    pub(super) fn build(root: &Path, cfg: &Config, tools: Option<&[Tool]>) -> Result<Self> {
        let specs = selected_specs(cfg, tools);
        let jobs = build_jobs(root, &specs)?;
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
        self.specs.iter().find(|spec| dest.starts_with(&spec.to))
    }
}

fn selected_specs(cfg: &Config, tools: Option<&[Tool]>) -> Vec<GenerateSpec> {
    cfg.generate
        .values()
        .filter(|spec| config::generate_selected(spec, tools))
        .cloned()
        .collect()
}

fn build_jobs(root: &Path, specs: &[GenerateSpec]) -> Result<Vec<Job>> {
    let mut jobs = Vec::new();
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
            let rel_no_ext = rel_to_from.with_extension("");
            let file_name = rel_no_ext
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let mut dest_sub = rel_no_ext.clone();
            dest_sub.set_file_name(format!("{file_name}{suffix}"));
            jobs.push(Job {
                format: spec.format,
                src_rel: spec.from.join(rel_to_from),
                dest_rel: spec.to.join(dest_sub),
            });
        }
    }
    jobs.sort_by(|a, b| a.dest_rel.cmp(&b.dest_rel));
    Ok(jobs)
}
