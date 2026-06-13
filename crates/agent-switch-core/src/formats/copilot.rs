//! GitHub Copilot agent, prompt, and instruction import/export.

use anyhow::Result;

use super::markdown::{
    self, apply_to_to_paths, base_with_namespace, canonical_with_tool_ns, paths_to_apply_to,
    render, set_string,
};

pub fn export_agent(source: &str) -> Result<String> {
    let doc = markdown::parse(source)?;
    let fm = base_with_namespace(&doc.frontmatter, "copilot", &["name", "description"]);
    render(fm, &doc.body)
}

pub fn import_agent(source: &str) -> Result<String> {
    let doc = markdown::parse(source)?;
    let fm = canonical_with_tool_ns(
        "copilot",
        &doc.frontmatter,
        &["name", "description"],
        &["name", "description"],
    );
    render(fm, &doc.body)
}

pub fn export_prompt(source: &str) -> Result<String> {
    let doc = markdown::parse(source)?;
    let fm = base_with_namespace(&doc.frontmatter, "copilot", &["name", "description"]);
    render(fm, &doc.body)
}

pub fn import_prompt(source: &str) -> Result<String> {
    // Prompt and agent formats are identical on import. Kept as a separate
    // function so any future divergence only requires changes here.
    import_agent(source)
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
