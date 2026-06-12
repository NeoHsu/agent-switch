use std::path::Path;

use anyhow::Result;

use super::markdown::{self, base_with_namespace, canonical_with_tool_ns, render, set_string};

pub fn export_agent(source: &str) -> Result<String> {
    let doc = markdown::parse(source)?;
    let mut fm = base_with_namespace(&doc.frontmatter, "opencode", &["description"]);
    set_string(&mut fm, "mode", "subagent");
    render(fm, &doc.body)
}

pub fn import_agent(path: &Path, source: &str) -> Result<String> {
    let doc = markdown::parse(source)?;
    let mut fm = canonical_with_tool_ns(
        "opencode",
        &doc.frontmatter,
        &["description"],
        &["description", "mode"],
    );
    // In serde_yml, Mapping::contains_key takes &str directly.
    if !fm.contains_key("name")
        && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
    {
        set_string(&mut fm, "name", stem);
    }
    render(fm, &doc.body)
}
