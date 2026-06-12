use std::{collections::BTreeMap, fs, path::Path};

use anyhow::Result;
use serde_json::{json, Value};
use toml_edit::{value, DocumentMut, Item, Table};

use crate::fs::write_if_changed;

const CODEX_START: &str = "# >>> agentstitch:mcp >>>";
const CODEX_END: &str = "# <<< agentstitch:mcp <<<";

pub fn merge_opencode(
    root: &Path,
    canonical_mcp: &Path,
    target: &Path,
    check: bool,
) -> Result<bool> {
    if !canonical_mcp.exists() {
        return Ok(false);
    }
    let canonical: Value = serde_json::from_str(&fs::read_to_string(canonical_mcp)?)?;
    let mut target_json = if target.exists() {
        serde_json::from_str::<Value>(&fs::read_to_string(target)?)?
    } else {
        json!({})
    };
    let obj = target_json.as_object_mut().expect("json object");
    obj.insert("mcp".into(), convert_opencode_mcp(&canonical));
    obj.entry("instructions").or_insert(json!([]));
    let text = format!("{}\n", serde_json::to_string_pretty(&target_json)?);
    if target.exists() && fs::read_to_string(target).unwrap_or_default() == text {
        return Ok(false);
    }
    if !check {
        write_if_changed(target, &text)?;
    }
    let _ = root;
    Ok(true)
}

pub fn merge_codex(canonical_mcp: &Path, target: &Path, check: bool) -> Result<bool> {
    if !canonical_mcp.exists() {
        return Ok(false);
    }
    let canonical: Value = serde_json::from_str(&fs::read_to_string(canonical_mcp)?)?;
    let block = render_codex_mcp_block(&canonical);
    let existing = fs::read_to_string(target).unwrap_or_default();
    let next = replace_marker_block(&existing, &block);
    if existing == next {
        return Ok(false);
    }
    if !check {
        write_if_changed(target, &next)?;
    }
    Ok(true)
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
    let Some(start) = existing.find(CODEX_START) else {
        if existing.trim().is_empty() {
            return block.to_string();
        }
        let mut next = existing.trim_end().to_string();
        next.push_str("\n\n");
        next.push_str(block);
        return next;
    };
    let Some(end_rel) = existing[start..].find(CODEX_END) else {
        let mut next = existing[..start].trim_end().to_string();
        next.push_str("\n\n");
        next.push_str(block);
        return next;
    };
    let end = start + end_rel + CODEX_END.len();
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

pub fn empty_mcp() -> String {
    let mut root = BTreeMap::new();
    root.insert("mcpServers", BTreeMap::<String, Value>::new());
    format!("{}\n", serde_json::to_string_pretty(&root).unwrap())
}
