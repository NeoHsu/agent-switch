//! Merge helpers for Model Context Protocol configuration files.

use std::{io, path::Path};

use anyhow::Result;
use serde_json::{Value, json};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::{
    Error,
    fs::{read_text, write_if_changed},
    tool::MergeFormat,
};

const CODEX_START: &str = "# >>> agent-switch:mcp >>>";
const CODEX_END: &str = "# <<< agent-switch:mcp <<<";

pub const EMPTY_MCP: &str = "{\n  \"mcpServers\": {}\n}\n";

pub fn merge(
    format: MergeFormat,
    canonical_mcp: &Path,
    target: &Path,
    check: bool,
) -> Result<bool> {
    match format {
        MergeFormat::Opencode => merge_opencode(canonical_mcp, target, check),
        MergeFormat::Codex => merge_codex(canonical_mcp, target, check),
    }
}

fn merge_opencode(canonical_mcp: &Path, target: &Path, check: bool) -> Result<bool> {
    if !canonical_mcp.exists() {
        return Ok(false);
    }
    let canonical: Value = serde_json::from_str(&read_text(canonical_mcp)?)?;
    let mut target_json = if target.exists() {
        serde_json::from_str::<Value>(&read_text(target)?)?
    } else {
        json!({})
    };
    let obj = target_json.as_object_mut().ok_or_else(|| {
        Error::Config(format!(
            "merge target is not a JSON object: {}",
            target.display()
        ))
    })?;
    obj.insert("mcp".into(), convert_opencode_mcp(&canonical));
    obj.entry("instructions").or_insert(json!([]));
    let text = format!("{}\n", serde_json::to_string_pretty(&target_json)?);
    if target.exists() && read_existing_text(target)? == text {
        return Ok(false);
    }
    if !check {
        write_if_changed(target, &text)?;
    }
    Ok(true)
}

fn merge_codex(canonical_mcp: &Path, target: &Path, check: bool) -> Result<bool> {
    if !canonical_mcp.exists() {
        return Ok(false);
    }
    let canonical: Value = serde_json::from_str(&read_text(canonical_mcp)?)?;
    let block = render_codex_mcp_block(&canonical);
    let existing = read_existing_text(target)?;
    let next = replace_marker_block(&existing, &block);
    if existing == next {
        return Ok(false);
    }
    if !check {
        write_if_changed(target, &next)?;
    }
    Ok(true)
}

fn read_existing_text(path: &Path) -> Result<String> {
    match read_text(path) {
        Ok(text) => Ok(text),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err.into()),
    }
}

fn convert_opencode_mcp(canonical: &Value) -> Value {
    let Some(servers) = canonical.get("mcpServers").and_then(Value::as_object) else {
        return json!({});
    };
    let mut out = serde_json::Map::new();
    for (name, server) in servers {
        if server.get("type").and_then(Value::as_str) == Some("http") || server.get("url").is_some()
        {
            out.insert(
                name.clone(),
                json!({
                    "type": "remote",
                    "url": server.get("url").cloned().unwrap_or(Value::String(String::new())),
                    "enabled": true,
                    "headers": server.get("headers").cloned().unwrap_or(json!({}))
                }),
            );
        } else {
            let mut cmd = vec![];
            if let Some(command) = server.get("command").and_then(Value::as_str) {
                cmd.push(Value::String(command.into()));
            }
            if let Some(args) = server.get("args").and_then(Value::as_array) {
                cmd.extend(args.iter().cloned());
            }
            out.insert(
                name.clone(),
                json!({
                    "type": "local",
                    "command": cmd,
                    "enabled": true,
                    "environment": server.get("env").cloned().unwrap_or(json!({}))
                }),
            );
        }
    }
    Value::Object(out)
}

fn render_codex_mcp_block(canonical: &Value) -> String {
    let mut doc = DocumentMut::new();
    if let Some(servers) = canonical.get("mcpServers").and_then(Value::as_object) {
        for (name, server) in servers {
            let mut table = Table::new();
            if let Some(command) = server.get("command").and_then(Value::as_str) {
                table["command"] = value(command);
            }
            if let Some(args) = server.get("args").and_then(Value::as_array) {
                let vals = args
                    .iter()
                    .filter_map(Value::as_str)
                    .map(toml_edit::Value::from);
                table["args"] = value(toml_edit::Array::from_iter(vals));
            }
            if let Some(env) = server.get("env").and_then(Value::as_object) {
                let mut inline = toml_edit::InlineTable::new();
                for (key, val) in env {
                    if let Some(s) = val.as_str() {
                        inline.insert(key, toml_edit::Value::from(s));
                    }
                }
                table["env"] = value(inline);
            }
            doc["mcp_servers"][name] = Item::Table(table);
        }
    }
    format!("{CODEX_START}\n{}{CODEX_END}\n", doc)
}

fn replace_marker_block(existing: &str, block: &str) -> String {
    let marker = existing.find(CODEX_START).map(|start| (start, CODEX_END));
    let Some((start, end_marker)) = marker else {
        if existing.trim().is_empty() {
            return block.to_string();
        }
        let mut next = existing.trim_end().to_string();
        next.push_str("\n\n");
        next.push_str(block);
        return next;
    };
    let Some(end_rel) = existing[start..].find(end_marker) else {
        let mut next = existing[..start].trim_end().to_string();
        next.push_str("\n\n");
        next.push_str(block);
        return next;
    };
    let end = start + end_rel + end_marker.len();
    let mut next = String::new();
    next.push_str(&existing[..start]);
    next.push_str(block.trim_end());
    next.push_str(&existing[end..]);
    if !next.ends_with('\n') {
        next.push('\n');
    }
    next
}

pub fn canonical_mcp_path(root: &Path, agents_dir: &Path) -> std::path::PathBuf {
    root.join(agents_dir).join("mcp.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block() -> &'static str {
        "# >>> agent-switch:mcp >>>\n[mcp_servers.demo]\ncommand = \"npx\"\n# <<< agent-switch:mcp <<<\n"
    }

    #[test]
    fn marker_block_replaces_current_markers() {
        let existing = "theme = \"dark\"\n\n# >>> agent-switch:mcp >>>\nold = true\n# <<< agent-switch:mcp <<<\n";
        let next = replace_marker_block(existing, block());

        assert!(next.contains("theme = \"dark\""));
        assert!(next.contains("[mcp_servers.demo]"));
        assert!(!next.contains("old = true"));
        assert!(next.ends_with('\n'));
    }

    #[test]
    fn marker_block_handles_missing_end_marker() {
        let existing = "theme = \"dark\"\n\n# >>> agent-switch:mcp >>>\nold = true\n";
        let next = replace_marker_block(existing, block());

        assert_eq!(
            next,
            "theme = \"dark\"\n\n# >>> agent-switch:mcp >>>\n[mcp_servers.demo]\ncommand = \"npx\"\n# <<< agent-switch:mcp <<<\n"
        );
    }

    #[test]
    fn marker_block_uses_block_for_empty_file() {
        assert_eq!(replace_marker_block("\n\n", block()), block());
    }

    #[test]
    fn codex_mcp_block_renders_command_args_and_env() {
        let canonical = json!({
            "mcpServers": {
                "context7": {
                    "command": "npx",
                    "args": ["-y", "@upstash/context7-mcp"],
                    "env": {"KEY": "${KEY}"}
                }
            }
        });

        let rendered = render_codex_mcp_block(&canonical);

        assert!(rendered.contains("mcp_servers"));
        assert!(rendered.contains("context7"));
        assert!(rendered.contains("command = \"npx\""));
        assert!(rendered.contains("args = [\"-y\", \"@upstash/context7-mcp\"]"));
        assert!(rendered.contains("env = { KEY = \"${KEY}\" }"));
    }
}
