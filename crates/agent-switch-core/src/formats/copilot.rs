//! GitHub Copilot agent, prompt, and instruction import/export.

use std::path::Path;

use anyhow::Result;

use crate::Error;

use super::markdown::{
    self, apply_to_to_paths, base_with_namespace, canonical_with_tool_ns, paths_to_apply_to,
    render, set_string, str_value,
};

pub fn export_agent(source: &str) -> Result<String> {
    let doc = markdown::parse(source)?;
    require_string(&doc.frontmatter, "name", "copilot-agent")?;
    require_string(&doc.frontmatter, "description", "copilot-agent")?;
    let fm = base_with_namespace(&doc.frontmatter, "copilot", &["name", "description"]);
    render(fm, &doc.body)
}

pub fn import_agent(path: &Path, source: &str) -> Result<String> {
    let doc = markdown::parse(source)?;
    let mut fm = canonical_with_tool_ns(
        "copilot",
        &doc.frontmatter,
        &["name", "description"],
        &["name", "description"],
    );
    infer_missing_name(path, ".agent.md", &mut fm);
    render(fm, &doc.body)
}

pub fn export_prompt(source: &str) -> Result<String> {
    let doc = markdown::parse(source)?;
    let fm = base_with_namespace(&doc.frontmatter, "copilot", &["name", "description"]);
    render(fm, &doc.body)
}

pub fn import_prompt(path: &Path, source: &str) -> Result<String> {
    // Prompt and agent formats are identical on import. Kept as a separate
    // function so any future divergence only requires changes here.
    let doc = markdown::parse(source)?;
    let mut fm = canonical_with_tool_ns(
        "copilot",
        &doc.frontmatter,
        &["name", "description"],
        &["name", "description"],
    );
    infer_missing_name(path, ".prompt.md", &mut fm);
    render(fm, &doc.body)
}

pub fn export_instructions(source: &str) -> Result<String> {
    let doc = markdown::parse(source)?;
    let mut fm = base_with_namespace(&doc.frontmatter, "copilot", &["description"]);
    let apply_to = paths_to_apply_to(&doc.frontmatter);
    set_string(&mut fm, "applyTo", apply_to);
    render(fm, &doc.body)
}

pub fn import_instructions(source: &str) -> Result<String> {
    let doc = markdown::parse(source)?;
    let mut fm = canonical_with_tool_ns(
        "copilot",
        &doc.frontmatter,
        &["description"],
        &["description", "applyTo"],
    );
    let apply_to = doc
        .frontmatter
        .get("applyTo")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned);
    apply_to_to_paths(apply_to, &mut fm);
    render(fm, &doc.body)
}

fn require_string(map: &noyalib::Mapping, key: &str, format: &str) -> Result<()> {
    match str_value(map, key) {
        Some(value) if !value.trim().is_empty() => Ok(()),
        _ => Err(Error::Config(format!("{format} requires `{key}`")).into()),
    }
}

pub fn native_basename(path: &Path, suffix: &str) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    file_name
        .strip_suffix(suffix)
        .map(ToOwned::to_owned)
        .or_else(|| path.file_stem()?.to_str().map(ToOwned::to_owned))
}

fn infer_missing_name(path: &Path, suffix: &str, fm: &mut noyalib::Mapping) {
    if str_value(fm, "name")
        .as_deref()
        .is_some_and(|name| !name.trim().is_empty())
    {
        return;
    }
    if let Some(name) = native_basename(path, suffix) {
        set_string(fm, "name", name);
    }
}
