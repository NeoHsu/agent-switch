//! Canonical MCP conversion into tool-native representations.

use super::*;
use toml_edit::{DocumentMut, Item, Table, value};

pub(super) fn convert_opencode_mcp(canonical: &Value) -> Value {
    let Some(servers) = canonical.get("mcpServers").and_then(Value::as_object) else {
        return json!({});
    };
    let mut out = serde_json::Map::new();
    for (name, server) in servers {
        let Some(server_obj) = server.as_object() else {
            continue;
        };
        let remote_url = server_obj
            .get("url")
            .or_else(|| server_obj.get("serverUrl"))
            .cloned();
        let is_remote = remote_url.is_some()
            || matches!(str_field(server, "type"), Some("http" | "sse" | "remote"));
        let mut cfg = serde_json::Map::new();
        if is_remote {
            cfg.insert("type".into(), json!("remote"));
            if let Some(url) = remote_url {
                cfg.insert("url".into(), url);
            }
            copy_json_key(server_obj, &mut cfg, "headers");
            copy_json_key(server_obj, &mut cfg, "oauth");
        } else {
            let mut command = Vec::new();
            if let Some(executable) = server_obj.get("command").and_then(Value::as_str) {
                command.push(Value::String(executable.into()));
            }
            if let Some(args) = server_obj.get("args").and_then(Value::as_array) {
                command.extend(args.iter().cloned());
            }
            cfg.insert("type".into(), json!("local"));
            cfg.insert("command".into(), Value::Array(command));
            if let Some(environment) = server_obj.get("env").cloned() {
                cfg.insert("environment".into(), environment);
            }
            copy_json_key(server_obj, &mut cfg, "cwd");
        }
        cfg.insert(
            "enabled".into(),
            server_obj.get("enabled").cloned().unwrap_or(json!(true)),
        );
        copy_json_key(server_obj, &mut cfg, "timeout");
        out.insert(name.clone(), Value::Object(cfg));
    }
    Value::Object(out)
}

pub(super) fn convert_copilot_mcp(canonical: &Value) -> Value {
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

pub(super) fn convert_antigravity_mcp(canonical: &Value) -> Value {
    let Some(servers) = canonical.get("mcpServers").and_then(Value::as_object) else {
        return json!({ "mcpServers": {} });
    };
    let mut out = serde_json::Map::new();
    for (name, server) in servers {
        let Some(server_obj) = server.as_object() else {
            continue;
        };
        let mut cfg = serde_json::Map::new();
        let remote_url = server_obj
            .get("serverUrl")
            .or_else(|| server_obj.get("url"))
            .or_else(|| server_obj.get("httpUrl"))
            .cloned();
        let is_remote = remote_url.is_some()
            || matches!(str_field(server, "type"), Some("http" | "sse" | "remote"));
        if is_remote {
            if let Some(url) = remote_url {
                cfg.insert("serverUrl".into(), url);
            }
            copy_json_key(server_obj, &mut cfg, "headers");
        } else {
            copy_json_key(server_obj, &mut cfg, "command");
            copy_json_key(server_obj, &mut cfg, "args");
            copy_json_key(server_obj, &mut cfg, "env");
            copy_json_key(server_obj, &mut cfg, "cwd");
        }
        for key in ["authProviderType", "oauth", "disabled", "disabledTools"] {
            copy_json_key(server_obj, &mut cfg, key);
        }
        if !cfg.contains_key("disabledTools") {
            if let Some(disabled_tools) = server_obj.get("disabled_tools").cloned() {
                cfg.insert("disabledTools".into(), disabled_tools);
            }
        }
        if !cfg.contains_key("disabled")
            && server_obj.get("enabled").and_then(Value::as_bool) == Some(false)
        {
            cfg.insert("disabled".into(), json!(true));
        }
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

pub(super) fn render_codex_mcp_block(canonical: &Value) -> String {
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

pub(super) fn replace_marker_block(existing: &str, block: &str) -> String {
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

pub(super) fn copy_json_key(
    source: &serde_json::Map<String, Value>,
    target: &mut serde_json::Map<String, Value>,
    key: &str,
) {
    if let Some(value) = source.get(key).cloned() {
        target.insert(key.into(), value);
    }
}
