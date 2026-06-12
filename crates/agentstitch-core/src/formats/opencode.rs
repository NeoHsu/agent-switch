use std::path::Path;

use anyhow::Result;
use serde_yaml::Value;

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
    if !fm.contains_key(Value::String("name".into())) {
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            set_string(&mut fm, "name", stem);
        }
    }
    render(fm, &doc.body)
}
