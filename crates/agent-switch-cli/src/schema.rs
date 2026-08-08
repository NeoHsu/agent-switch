//! Bundled machine-readable schema discovery for agents and scripts.

use agent_switch_core::{CommandOutput, output};
use anyhow::Result;
use serde_json::json;

struct SchemaEntry {
    name: &'static str,
    description: &'static str,
    content: &'static str,
}

const SCHEMAS: &[SchemaEntry] = &[SchemaEntry {
    name: "cli-output-v1",
    description: "Shared machine-readable CLI response envelope.",
    content: include_str!("../../../schema/cli-output-v1.json"),
}];

pub(crate) fn list(json_output: bool) -> Result<CommandOutput> {
    let mut out = CommandOutput::default();
    if json_output {
        let schemas = SCHEMAS
            .iter()
            .map(|schema| {
                json!({
                    "name": schema.name,
                    "description": schema.description,
                })
            })
            .collect::<Vec<_>>();
        out.push(output::render_json(&json!({ "schemas": schemas }))?);
    } else {
        for schema in SCHEMAS {
            out.push(format!("{}\t{}", schema.name, schema.description));
        }
    }
    Ok(out)
}

pub(crate) fn content(name: &str) -> Option<&'static str> {
    let name = name.strip_suffix(".schema.json").unwrap_or(name);
    SCHEMAS
        .iter()
        .find(|schema| schema.name == name)
        .map(|schema| schema.content)
}
