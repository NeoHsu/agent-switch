use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use walkdir::WalkDir;

use crate::{
    config::{self, Config, GenerateSpec},
    formats,
    fs::{abs, copy_file, is_fake_symlink, relative_link, repo_path, write_if_changed},
    manifest::{self, GeneratedEntry},
    mcp, CommandOutput, ExitCode,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncOptions {
    pub check: bool,
    pub import_only: bool,
    pub export_only: bool,
}

#[derive(Debug, Clone)]
struct Job {
    id: String,
    format: String,
    src_rel: PathBuf,
    dest_rel: PathBuf,
}

pub fn run(
    root: &Path,
    cfg: &Config,
    tools: Option<&[String]>,
    opts: SyncOptions,
) -> Result<CommandOutput> {
    let mut out = CommandOutput::default();
    let manifest_path = abs(root, &cfg.manifest);
    let mut manifest = manifest::load(&manifest_path).context("error: failed to read manifest")?;
    let specs = selected_specs(cfg, tools);
    let jobs = build_jobs(root, &specs)?;
    let job_dests = jobs
        .iter()
        .map(|job| repo_path(&job.dest_rel))
        .collect::<BTreeSet<_>>();
    let mut changed = false;

    if !opts.export_only {
        changed |= import_changed(root, &mut manifest, &specs, opts.check, &mut out)?;
    }

    if !opts.import_only {
        changed |= export_jobs(root, &mut manifest, &jobs, opts.check, &mut out)?;
        changed |= remove_stale(
            root,
            &mut manifest,
            &specs,
            &job_dests,
            opts.check,
            &mut out,
        )?;
        changed |= sync_link_copies(root, cfg, tools, &mut manifest, opts.check, &mut out)?;
        changed |= merge_configs(root, cfg, tools, opts.check, &mut out)?;
    }

    if opts.check {
        if changed {
            out.push("--check: drift detected; run `agentstitch sync`");
            out.exit = Some(ExitCode::Drift);
        } else {
            out.push("synced, no changes.");
        }
        return Ok(out);
    }

    manifest::save(&manifest_path, &mut manifest)?;
    if !changed && out.lines.is_empty() {
        out.push("synced, no changes.");
    }
    Ok(out)
}

fn selected_specs(cfg: &Config, tools: Option<&[String]>) -> Vec<(String, GenerateSpec)> {
    cfg.generate
        .iter()
        .filter(|(id, spec)| config::generate_selected(id, spec, tools))
        .map(|(id, spec)| (id.clone(), spec.clone()))
        .collect()
}

fn build_jobs(root: &Path, specs: &[(String, GenerateSpec)]) -> Result<Vec<Job>> {
    let mut jobs = Vec::new();
    for (id, spec) in specs {
        let from_abs = abs(root, &spec.from);
        if !from_abs.exists() {
            continue;
        }
        let suffix = spec.suffix.clone().unwrap_or_default();
        for entry in WalkDir::new(&from_abs).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
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
                id: id.clone(),
                format: spec.format.clone(),
                src_rel: spec.from.join(rel_to_from),
                dest_rel: spec.to.join(dest_sub),
            });
        }
    }
    jobs.sort_by(|a, b| a.dest_rel.cmp(&b.dest_rel));
    Ok(jobs)
}

fn import_changed(
    root: &Path,
    manifest: &mut manifest::Manifest,
    specs: &[(String, GenerateSpec)],
    check: bool,
    out: &mut CommandOutput,
) -> Result<bool> {
    let mut changed = false;
    let generated_entries = manifest.generated.clone();
    for (dest, entry) in generated_entries {
        let dest_rel = PathBuf::from(&dest);
        let Some(spec) = spec_for_dest(specs, &dest_rel) else {
            continue;
        };
        let dest_abs = abs(root, &dest_rel);
        if !dest_abs.exists() {
            continue;
        }
        let generated_text = fs::read_to_string(&dest_abs)?;
        let generated_hash = manifest::sha256_text(&generated_text);
        if generated_hash == entry.hash {
            continue;
        }
        let src_rel = PathBuf::from(&entry.src);
        let src_abs = abs(root, &src_rel);
        let src_changed = src_abs
            .exists()
            .then(|| fs::read_to_string(&src_abs).ok())
            .flatten()
            .map(|text| manifest::sha256_text(&text) != entry.src_hash)
            .unwrap_or(false);
        let mut canonical = formats::import(&spec.format, &dest_rel, &generated_text)?;
        if let Some(existing) = src_abs
            .exists()
            .then(|| fs::read_to_string(&src_abs).ok())
            .flatten()
        {
            canonical = preserve_existing_canonical_fields(&existing, &canonical, &spec.format)?;
        }
        let src_hash = manifest::sha256_text(&canonical);
        changed = true;
        if !check {
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
        if src_changed {
            out.push(format!(
                "imported(conflict, tool-side wins): {} -> {}",
                dest,
                repo_path(&src_rel)
            ));
        } else {
            out.push(format!("imported: {} -> {}", dest, repo_path(&src_rel)));
        }
    }
    Ok(changed)
}

fn export_jobs(
    root: &Path,
    manifest: &mut manifest::Manifest,
    jobs: &[Job],
    check: bool,
    out: &mut CommandOutput,
) -> Result<bool> {
    let mut changed = false;
    for job in jobs {
        let src_abs = abs(root, &job.src_rel);
        let dest_abs = abs(root, &job.dest_rel);
        let source = fs::read_to_string(&src_abs)?;
        let generated = formats::export(&job.format, &job.src_rel, &source)
            .with_context(|| format!("error: failed to export {}", repo_path(&job.src_rel)))?;
        let generated_hash = manifest::sha256_text(&generated);
        let src_hash = manifest::sha256_text(&source);
        let dest_key = repo_path(&job.dest_rel);
        let write_needed =
            !dest_abs.exists() || fs::read_to_string(&dest_abs).unwrap_or_default() != generated;
        if write_needed {
            changed = true;
            if !check {
                write_if_changed(&dest_abs, &generated)?;
            }
            out.push(format!("generated: {}", dest_key));
        }
        if !check {
            manifest.generated.insert(
                dest_key,
                GeneratedEntry {
                    hash: generated_hash,
                    src: repo_path(&job.src_rel),
                    src_hash,
                },
            );
        }
        let _ = &job.id;
    }
    Ok(changed)
}

fn remove_stale(
    root: &Path,
    manifest: &mut manifest::Manifest,
    specs: &[(String, GenerateSpec)],
    current_dests: &BTreeSet<String>,
    check: bool,
    out: &mut CommandOutput,
) -> Result<bool> {
    let mut changed = false;
    let keys = manifest.generated.keys().cloned().collect::<Vec<_>>();
    for key in keys {
        if current_dests.contains(&key) {
            continue;
        }
        let dest_rel = PathBuf::from(&key);
        if spec_for_dest(specs, &dest_rel).is_none() {
            continue;
        }
        changed = true;
        if !check {
            let dest_abs = abs(root, &dest_rel);
            if dest_abs.exists() {
                fs::remove_file(dest_abs)?;
            }
            manifest.generated.remove(&key);
        }
        out.push(format!("removed: {key}"));
    }
    Ok(changed)
}

fn sync_link_copies(
    root: &Path,
    cfg: &Config,
    tools: Option<&[String]>,
    manifest: &mut manifest::Manifest,
    check: bool,
    out: &mut CommandOutput,
) -> Result<bool> {
    let mut changed = false;
    for (link, target) in &cfg.symlinks {
        if !config::symlink_selected(link, target, tools) {
            continue;
        }
        let link_rel = Path::new(link);
        let target_rel = Path::new(target);
        let link_abs = abs(root, link_rel);
        let target_abs = abs(root, target_rel);
        if link_abs.is_symlink() || !target_abs.is_file() {
            continue;
        }
        let rel_target = relative_link(&link_abs, &target_abs);
        if is_fake_symlink(&link_abs, &rel_target, target) {
            out.push(format!(
                "warning: {} is an unrestored git symlink placeholder; run `agentstitch setup`",
                repo_path(link_rel)
            ));
            continue;
        }
        if !link_abs.exists() && target_abs.exists() {
            changed = true;
            let target_text = fs::read(&target_abs)?;
            if !check {
                copy_file(&target_abs, &link_abs)?;
                manifest
                    .links
                    .insert(repo_path(link_rel), manifest::sha256_bytes(&target_text));
            }
            out.push(format!(
                "copied: {} -> {}",
                repo_path(target_rel),
                repo_path(link_rel)
            ));
            continue;
        }
        if !link_abs.is_file() {
            continue;
        }
        let link_bytes = fs::read(&link_abs)?;
        let target_bytes = fs::read(&target_abs)?;
        let current_hash = manifest::sha256_bytes(&link_bytes);
        let tracked = manifest.links.get(&repo_path(link_rel)).cloned();
        if tracked.as_deref() != Some(&current_hash) {
            changed = true;
            if !check {
                copy_file(&link_abs, &target_abs)?;
                manifest.links.insert(repo_path(link_rel), current_hash);
            }
            out.push(format!(
                "copied: {} -> {}",
                repo_path(link_rel),
                repo_path(target_rel)
            ));
        } else if link_bytes != target_bytes {
            changed = true;
            let target_hash = manifest::sha256_bytes(&target_bytes);
            if !check {
                copy_file(&target_abs, &link_abs)?;
                manifest.links.insert(repo_path(link_rel), target_hash);
            }
            out.push(format!(
                "copied: {} -> {}",
                repo_path(target_rel),
                repo_path(link_rel)
            ));
        }
    }
    Ok(changed)
}

fn merge_configs(
    root: &Path,
    cfg: &Config,
    tools: Option<&[String]>,
    check: bool,
    out: &mut CommandOutput,
) -> Result<bool> {
    let mut changed = false;
    let canonical_mcp = mcp::canonical_mcp_path(root, &cfg.agents_dir);
    for (id, spec) in &cfg.merge {
        if !config::merge_selected(id, spec, tools) {
            continue;
        }
        let target = abs(root, &spec.to);
        let did_change = if id == "opencode-config" || spec.to == PathBuf::from("opencode.json") {
            mcp::merge_opencode(root, &canonical_mcp, &target, check)?
        } else if id == "codex-config" || spec.to == PathBuf::from(".codex/config.toml") {
            mcp::merge_codex(&canonical_mcp, &target, check)?
        } else {
            false
        };
        if did_change {
            changed = true;
            out.push(format!("merged: {}", repo_path(&spec.to)));
        }
    }
    Ok(changed)
}

fn spec_for_dest<'a>(specs: &'a [(String, GenerateSpec)], dest: &Path) -> Option<&'a GenerateSpec> {
    specs.iter().find_map(|(_, spec)| {
        if dest.starts_with(&spec.to) {
            Some(spec)
        } else {
            None
        }
    })
}

fn preserve_existing_canonical_fields(
    existing: &str,
    imported: &str,
    format: &str,
) -> Result<String> {
    use serde_yaml::Value;

    let existing_doc = formats::markdown::parse(existing)?;
    let mut imported_doc = formats::markdown::parse(imported)?;
    let current_tool = match format {
        "copilot-agent" | "copilot-prompt" | "copilot-instructions" => Some("copilot"),
        "opencode-agent" => Some("opencode"),
        "codex-agent" => Some("codex"),
        _ => None,
    };
    for (key, value) in existing_doc.frontmatter {
        let Some(key_str) = key.as_str() else {
            continue;
        };
        let preserve = matches!(key_str, "tools" | "model")
            || matches!(key_str, "copilot" | "opencode" | "codex") && Some(key_str) != current_tool;
        if preserve && !imported_doc.frontmatter.contains_key(&key) {
            imported_doc
                .frontmatter
                .insert(Value::String(key_str.into()), value);
        }
    }
    formats::markdown::render(imported_doc.frontmatter, &imported_doc.body)
}

#[allow(dead_code)]
fn _debug_jobs(jobs: &[Job]) -> BTreeMap<String, String> {
    jobs.iter()
        .map(|j| (repo_path(&j.dest_rel), repo_path(&j.src_rel)))
        .collect()
}
