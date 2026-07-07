//! OpenCode agent import/export.

use std::path::Path;

use anyhow::Result;

use super::markdown::{
    self, base_with_namespace, canonical_with_tool_ns, merge_mapping, render, set_string,
};

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
    if fm.contains_key("name") {
        return render(fm, &doc.body);
    }
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        let mut with_name = serde_norway::Mapping::new();
        set_string(&mut with_name, "name", stem);
        merge_mapping(&mut with_name, fm);
        fm = with_name;
    }
    render(fm, &doc.body)
}
