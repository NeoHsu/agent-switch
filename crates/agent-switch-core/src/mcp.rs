//! Merge helpers for Model Context Protocol configuration files.

use std::{fs, io, path::Path};

use anyhow::Result;
use serde_json::{Value, json};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::{
    Error,
    fs::{io_error, read_text, write_if_changed},
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

/// Result of pruning agent-switch managed MCP content from a merge target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneOutcome {
    /// The whole file was agent-switch managed and has been removed.
    Removed,
    /// Managed content was stripped; user-owned content was kept in place.
    Cleaned,
    /// The file exists but is not recognizable as agent-switch output.
    Unmanaged,
    /// Nothing managed was present; no action needed.
    Absent,
}

/// Remove agent-switch managed MCP content from a merge target when the
/// owning tool is no longer selected. Only content this tool can prove it
/// generated is touched; anything else is reported as unmanaged.
pub fn prune(
    format: MergeFormat,
    canonical_mcp: &Path,
    target: &Path,
    check: bool,
) -> Result<PruneOutcome> {
    if !target.exists() {
        return Ok(PruneOutcome::Absent);
    }
    match format {
        MergeFormat::Opencode => prune_opencode(target, check),
        MergeFormat::Codex => prune_codex(target, check),
        MergeFormat::Copilot => prune_copilot(canonical_mcp, target, check),
    }
}

fn prune_opencode(target: &Path, check: bool) -> Result<PruneOutcome> {
    let Ok(mut existing) = serde_json::from_str::<Value>(&read_text(target)?) else {
        return Ok(PruneOutcome::Unmanaged);
    };
    let Some(obj) = existing.as_object_mut() else {
        return Ok(PruneOutcome::Unmanaged);
    };
    if obj.remove("mcp").is_none() {
        return Ok(PruneOutcome::Absent);
    }
    let only_empty_instructions = obj.len() == 1
        && obj
            .get("instructions")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty);
    if obj.is_empty() || only_empty_instructions {
        if !check {
            fs::remove_file(target).map_err(|err| io_error("remove merge target", target, err))?;
        }
        return Ok(PruneOutcome::Removed);
    }
    if !check {
        let text = format!("{}\n", serde_json::to_string_pretty(&existing)?);
        write_if_changed(target, &text)?;
    }
    Ok(PruneOutcome::Cleaned)
}

fn prune_codex(target: &Path, check: bool) -> Result<PruneOutcome> {
    let existing = read_text(target)?;
    let Some(start) = existing.find(CODEX_START) else {
        return Ok(PruneOutcome::Absent);
    };
    let Some(end_rel) = existing[start..].find(CODEX_END) else {
        // A start marker without an end marker means the block was edited by
        // hand; refuse to guess where managed content stops.
        return Ok(PruneOutcome::Unmanaged);
    };
    let mut end = start + end_rel + CODEX_END.len();
    if existing[end..].starts_with('\n') {
        end += 1;
    }
    let remainder = format!("{}{}", &existing[..start], &existing[end..]);
    if remainder.trim().is_empty() {
        if !check {
            fs::remove_file(target).map_err(|err| io_error("remove merge target", target, err))?;
        }
        return Ok(PruneOutcome::Removed);
    }
    if !check {
        let text = format!("{}\n", remainder.trim_end());
        write_if_changed(target, &text)?;
    }
    Ok(PruneOutcome::Cleaned)
}

fn prune_copilot(canonical_mcp: &Path, target: &Path, check: bool) -> Result<PruneOutcome> {
    // `.copilot/mcp-config.json` is wholly generated, so it is only removed
    // when it still matches what this tool would generate from the canonical
    // config; any deviation means user edits we must not delete.
    if !canonical_mcp.exists() {
        return Ok(PruneOutcome::Unmanaged);
    }
    let canonical: Value = serde_json::from_str(&read_text(canonical_mcp)?)?;
    let expected = format!(
        "{}\n",
        serde_json::to_string_pretty(&convert_copilot_mcp(&canonical))?
    );
    if read_text(target)? != expected {
        return Ok(PruneOutcome::Unmanaged);
    }
    if !check {
        fs::remove_file(target).map_err(|err| io_error("remove merge target", target, err))?;
    }
    Ok(PruneOutcome::Removed)
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

pub fn import_native(format: MergeFormat, target: &Path) -> Result<Option<Value>> {
    if !target.exists() {
        return Ok(None);
    }
    let text = read_text(target)?;
    let canonical = match format {
        MergeFormat::Opencode => import_opencode_mcp(&text)?,
        MergeFormat::Codex => import_codex_mcp(&text)?,
        MergeFormat::Copilot => import_copilot_mcp(&text)?,
    };
    Ok(Some(canonical))
}

fn import_copilot_mcp(source: &str) -> Result<Value> {
    let native: Value = serde_json::from_str(source)?;
    let Some(servers) = native.get("mcpServers").and_then(Value::as_object) else {
        return Ok(json!({ "mcpServers": {} }));
    };
    let mut out = serde_json::Map::new();
    for (name, server) in servers {
        let mut cfg = server.as_object().cloned().unwrap_or_default();
        cfg.remove("tools");
        if cfg.get("type").and_then(Value::as_str) == Some("local") {
            cfg.remove("type");
        }
        out.insert(name.clone(), Value::Object(cfg));
    }
    Ok(json!({ "mcpServers": out }))
}

fn import_opencode_mcp(source: &str) -> Result<Value> {
    let native: Value = serde_json::from_str(source)?;
    let Some(servers) = native.get("mcp").and_then(Value::as_object) else {
        return Ok(json!({ "mcpServers": {} }));
    };
    let mut out = serde_json::Map::new();
    for (name, server) in servers {
        let Some(server_obj) = server.as_object() else {
            continue;
        };
        let mut cfg = serde_json::Map::new();
        let is_remote = server_obj.get("type").and_then(Value::as_str) == Some("remote")
            || server_obj.contains_key("url");
        if is_remote {
            if let Some(url) = server_obj.get("url").cloned() {
                cfg.insert("url".into(), url);
            }
            if let Some(headers) = server_obj.get("headers").cloned() {
                cfg.insert("headers".into(), headers);
            }
            cfg.insert("type".into(), json!("http"));
        } else {
            import_opencode_command(server_obj.get("command"), &mut cfg);
            if let Some(env) = server_obj
                .get("environment")
                .or_else(|| server_obj.get("env"))
                .cloned()
            {
                cfg.insert("env".into(), env);
            }
        }
        copy_json_key(server_obj, &mut cfg, "enabled");
        copy_json_key(server_obj, &mut cfg, "required");
        copy_json_key(server_obj, &mut cfg, "startup_timeout_sec");
        copy_json_key(server_obj, &mut cfg, "tool_timeout_sec");
        out.insert(name.clone(), Value::Object(cfg));
    }
    Ok(json!({ "mcpServers": out }))
}

fn import_opencode_command(command: Option<&Value>, cfg: &mut serde_json::Map<String, Value>) {
    let Some(command) = command else {
        return;
    };
    if let Some(parts) = command.as_array() {
        if let Some(first) = parts.first().and_then(Value::as_str) {
            cfg.insert("command".into(), json!(first));
            if parts.len() > 1 {
                cfg.insert("args".into(), Value::Array(parts[1..].to_vec()));
            }
        }
    } else if let Some(command) = command.as_str() {
        cfg.insert("command".into(), json!(command));
    }
}

fn copy_json_key(
    source: &serde_json::Map<String, Value>,
    target: &mut serde_json::Map<String, Value>,
    key: &str,
) {
    if let Some(value) = source.get(key).cloned() {
        target.insert(key.into(), value);
    }
}

fn import_codex_mcp(source: &str) -> Result<Value> {
    let doc = source.parse::<DocumentMut>()?;
    let Some(mcp_servers) = doc.get("mcp_servers").and_then(Item::as_table) else {
        return Ok(json!({ "mcpServers": {} }));
    };
    let mut out = serde_json::Map::new();
    for (name, item) in mcp_servers.iter() {
        let Some(table) = item.as_table() else {
            continue;
        };
        let mut cfg = serde_json::Map::new();
        if let Some(url) = toml_str(table, "url") {
            cfg.insert("url".into(), json!(url));
            copy_toml_string(table, &mut cfg, "bearer_token_env_var");
            copy_toml_as(table, &mut cfg, "http_headers", "headers");
            copy_toml_as(table, &mut cfg, "env_http_headers", "env_http_headers");
        } else if let Some(command) = toml_str(table, "command") {
            cfg.insert("command".into(), json!(command));
            copy_toml_as(table, &mut cfg, "args", "args");
            copy_toml_as(table, &mut cfg, "env", "env");
            copy_toml_as(table, &mut cfg, "env_vars", "env_vars");
            copy_toml_string(table, &mut cfg, "cwd");
            copy_toml_string(table, &mut cfg, "experimental_environment");
        }
        copy_toml_as(table, &mut cfg, "enabled", "enabled");
        copy_toml_as(table, &mut cfg, "required", "required");
        copy_toml_as(
            table,
            &mut cfg,
            "startup_timeout_sec",
            "startup_timeout_sec",
        );
        copy_toml_as(table, &mut cfg, "tool_timeout_sec", "tool_timeout_sec");
        copy_toml_string(table, &mut cfg, "default_tools_approval_mode");
        copy_toml_as(table, &mut cfg, "enabled_tools", "enabled_tools");
        copy_toml_as(table, &mut cfg, "disabled_tools", "disabled_tools");
        out.insert(name.to_string(), Value::Object(cfg));
    }
    Ok(json!({ "mcpServers": out }))
}

fn toml_str(table: &Table, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(Item::as_value)
        .and_then(toml_edit::Value::as_str)
        .map(ToOwned::to_owned)
}

fn copy_toml_string(table: &Table, target: &mut serde_json::Map<String, Value>, key: &str) {
    if let Some(value) = toml_str(table, key) {
        target.insert(key.into(), json!(value));
    }
}

fn copy_toml_as(
    table: &Table,
    target: &mut serde_json::Map<String, Value>,
    source_key: &str,
    target_key: &str,
) {
    if let Some(value) = table.get(source_key).and_then(toml_item_to_json) {
        target.insert(target_key.into(), value);
    }
}

fn toml_item_to_json(item: &Item) -> Option<Value> {
    if let Some(value) = item.as_value() {
        Some(toml_value_to_json(value))
    } else if let Some(table) = item.as_table() {
        Some(Value::Object(toml_table_to_json(table)))
    } else {
        item.as_array_of_tables().map(|array| {
            Value::Array(
                array
                    .iter()
                    .map(|table| Value::Object(toml_table_to_json(table)))
                    .collect(),
            )
        })
    }
}

fn toml_value_to_json(value: &toml_edit::Value) -> Value {
    if let Some(s) = value.as_str() {
        Value::String(s.to_string())
    } else if let Some(b) = value.as_bool() {
        Value::Bool(b)
    } else if let Some(i) = value.as_integer() {
        Value::Number(i.into())
    } else if let Some(f) = value.as_float() {
        serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(f.to_string()))
    } else if let Some(arr) = value.as_array() {
        Value::Array(arr.iter().map(toml_value_to_json).collect())
    } else if let Some(table) = value.as_inline_table() {
        let mut out = serde_json::Map::new();
        for (key, value) in table.iter() {
            out.insert(key.to_string(), toml_value_to_json(value));
        }
        Value::Object(out)
    } else if let Some(datetime) = value.as_datetime() {
        Value::String(datetime.to_string())
    } else {
        Value::String(value.to_string())
    }
}

fn toml_table_to_json(table: &Table) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    for (key, item) in table.iter() {
        if let Some(value) = toml_item_to_json(item) {
            out.insert(key.to_string(), value);
        }
    }
    out
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
