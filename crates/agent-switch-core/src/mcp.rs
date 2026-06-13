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
        MergeFormat::Copilot => merge_copilot(canonical_mcp, target, check),
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

fn merge_copilot(canonical_mcp: &Path, target: &Path, check: bool) -> Result<bool> {
    if !canonical_mcp.exists() {
        return Ok(false);
    }
    let canonical: Value = serde_json::from_str(&read_text(canonical_mcp)?)?;
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&convert_copilot_mcp(&canonical))?
    );
    if target.exists() && read_existing_text(target)? == text {
        return Ok(false);
    }
    if !check {
        write_if_changed(target, &text)?;
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

fn convert_copilot_mcp(canonical: &Value) -> Value {
    let Some(servers) = canonical.get("mcpServers").and_then(Value::as_object) else {
        return json!({ "mcpServers": {} });
    };
    let mut out = serde_json::Map::new();
    for (name, server) in servers {
        let mut cfg = serde_json::Map::new();
        if server.get("url").is_some()
            || matches!(str_field(server, "type"), Some("http" | "sse" | "remote"))
        {
            cfg.insert("type".into(), json!(copilot_remote_type(server)));
            if let Some(url) = server.get("url").cloned() {
                cfg.insert("url".into(), url);
            }
            let headers = copilot_headers(server);
            if !headers.is_empty() {
                cfg.insert("headers".into(), Value::Object(headers));
            }
        } else {
            cfg.insert("type".into(), json!(copilot_local_type(server)));
            if let Some(command) = server.get("command").cloned() {
                cfg.insert("command".into(), command);
            }
            cfg.insert(
                "args".into(),
                server.get("args").cloned().unwrap_or_else(|| json!([])),
            );
            cfg.insert(
                "env".into(),
                server.get("env").cloned().unwrap_or_else(|| json!({})),
            );
        }
        cfg.insert("tools".into(), tool_list(server));
        out.insert(name.clone(), Value::Object(cfg));
    }
    json!({ "mcpServers": out })
}

fn str_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn copilot_remote_type(server: &Value) -> &'static str {
    match str_field(server, "type") {
        Some("sse") => "sse",
        _ => "http",
    }
}

fn copilot_local_type(server: &Value) -> &'static str {
    match str_field(server, "type") {
        Some("stdio") => "stdio",
        _ => "local",
    }
}

fn copilot_headers(server: &Value) -> serde_json::Map<String, Value> {
    let mut headers = server
        .get("headers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(env_headers) = server.get("env_http_headers").and_then(Value::as_object) {
        for (header, env_name) in env_headers {
            if let Some(env_name) = env_name.as_str() {
                headers
                    .entry(header.clone())
                    .or_insert_with(|| Value::String(format!("${{{env_name}}}")));
            }
        }
    }
    headers
}

fn tool_list(server: &Value) -> Value {
    if let Some(tools) = server.get("tools").and_then(Value::as_array) {
        return Value::Array(tools.clone());
    }
    if let Some(tools) = server.get("enabled_tools").and_then(Value::as_array) {
        return Value::Array(tools.clone());
    }
    if let Some(tool) = server.get("tools").and_then(Value::as_str) {
        return json!([tool]);
    }
    json!(["*"])
}

fn render_codex_mcp_block(canonical: &Value) -> String {
    let mut doc = DocumentMut::new();
    if let Some(servers) = canonical.get("mcpServers").and_then(Value::as_object) {
        let mut mcp_servers = Table::new();
        for (name, server) in servers {
            let mut table = Table::new();
            if let Some(url) = server.get("url").and_then(Value::as_str) {
                table["url"] = value(url);
                if let Some(token) = server.get("bearer_token_env_var").and_then(Value::as_str) {
                    table["bearer_token_env_var"] = value(token);
                }
                if let Some(headers) = server.get("headers").and_then(Value::as_object) {
                    table["http_headers"] = value(string_map(headers));
                }
                if let Some(headers) = server.get("env_http_headers").and_then(Value::as_object) {
                    table["env_http_headers"] = value(string_map(headers));
                }
            } else if let Some(command) = server.get("command").and_then(Value::as_str) {
                table["command"] = value(command);
                if let Some(args) = server.get("args").and_then(Value::as_array) {
                    table["args"] = value(string_array(args));
                }
                if let Some(env) = server.get("env").and_then(Value::as_object) {
                    table["env"] = value(string_map(env));
                }
                if let Some(env_vars) = server.get("env_vars").and_then(Value::as_array) {
                    table["env_vars"] = value(string_array(env_vars));
                }
                if let Some(cwd) = server.get("cwd").and_then(Value::as_str) {
                    table["cwd"] = value(cwd);
                }
                if let Some(env) = server
                    .get("experimental_environment")
                    .and_then(Value::as_str)
                {
                    table["experimental_environment"] = value(env);
                }
            }
            copy_bool(server, &mut table, "enabled");
            copy_bool(server, &mut table, "required");
            copy_i64(server, &mut table, "startup_timeout_sec");
            copy_i64(server, &mut table, "tool_timeout_sec");
            copy_string(server, &mut table, "default_tools_approval_mode");
            copy_string_array(server, &mut table, "enabled_tools");
            copy_string_array(server, &mut table, "disabled_tools");
            mcp_servers[name] = Item::Table(table);
        }
        doc["mcp_servers"] = Item::Table(mcp_servers);
    }
    format!("{CODEX_START}\n{}{CODEX_END}\n", doc)
}

fn string_array(values: &[Value]) -> toml_edit::Array {
    let vals = values
        .iter()
        .filter_map(Value::as_str)
        .map(toml_edit::Value::from);
    toml_edit::Array::from_iter(vals)
}

fn string_map(values: &serde_json::Map<String, Value>) -> toml_edit::InlineTable {
    let mut inline = toml_edit::InlineTable::new();
    for (key, val) in values {
        if let Some(s) = val.as_str() {
            inline.insert(key, toml_edit::Value::from(s));
        }
    }
    inline
}

fn copy_bool(source: &Value, table: &mut Table, key: &str) {
    if let Some(v) = source.get(key).and_then(Value::as_bool) {
        table[key] = value(v);
    }
}

fn copy_i64(source: &Value, table: &mut Table, key: &str) {
    if let Some(v) = source.get(key).and_then(Value::as_i64) {
        table[key] = value(v);
    }
}

fn copy_string(source: &Value, table: &mut Table, key: &str) {
    if let Some(v) = source.get(key).and_then(Value::as_str) {
        table[key] = value(v);
    }
}

fn copy_string_array(source: &Value, table: &mut Table, key: &str) {
    if let Some(v) = source.get(key).and_then(Value::as_array) {
        table[key] = value(string_array(v));
    }
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

    #[test]
    fn codex_mcp_block_renders_http_servers_and_tool_policy() {
        let canonical = json!({
            "mcpServers": {
                "figma": {
                    "url": "https://mcp.figma.com/mcp",
                    "bearer_token_env_var": "FIGMA_TOKEN",
                    "headers": {"X-Figma-Region": "us-east-1"},
                    "enabled_tools": ["inspect"],
                    "disabled_tools": ["write"],
                    "startup_timeout_sec": 20,
                    "enabled": true
                }
            }
        });

        let rendered = render_codex_mcp_block(&canonical);

        assert!(rendered.contains("[mcp_servers.figma]"));
        assert!(rendered.contains("url = \"https://mcp.figma.com/mcp\""));
        assert!(rendered.contains("bearer_token_env_var = \"FIGMA_TOKEN\""));
        assert!(rendered.contains("http_headers = { X-Figma-Region = \"us-east-1\" }"));
        assert!(rendered.contains("enabled_tools = [\"inspect\"]"));
        assert!(rendered.contains("disabled_tools = [\"write\"]"));
        assert!(rendered.contains("startup_timeout_sec = 20"));
        assert!(rendered.contains("enabled = true"));
    }

    #[test]
    fn copilot_mcp_conversion_adds_required_type_and_tools() {
        let canonical = json!({
            "mcpServers": {
                "playwright": {
                    "command": "npx",
                    "args": ["@playwright/mcp@latest"],
                    "env": {"KEY": "${KEY}"}
                },
                "context7": {
                    "type": "http",
                    "url": "https://mcp.context7.com/mcp",
                    "headers": {"CONTEXT7_API_KEY": "${COPILOT_MCP_CONTEXT7_API_KEY}"},
                    "tools": ["resolve-library-id"]
                }
            }
        });

        let converted = convert_copilot_mcp(&canonical);

        assert_eq!(
            converted["mcpServers"]["playwright"]["type"],
            json!("local")
        );
        assert_eq!(converted["mcpServers"]["playwright"]["tools"], json!(["*"]));
        assert_eq!(
            converted["mcpServers"]["playwright"]["args"],
            json!(["@playwright/mcp@latest"])
        );
        assert_eq!(converted["mcpServers"]["context7"]["type"], json!("http"));
        assert_eq!(
            converted["mcpServers"]["context7"]["tools"],
            json!(["resolve-library-id"])
        );
    }
}
