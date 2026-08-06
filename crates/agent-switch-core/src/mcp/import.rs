//! Native MCP configuration import adapters.

use super::convert::copy_json_key;
use super::*;
use toml_edit::{DocumentMut, Item, Table};

pub(super) fn import_copilot_mcp(source: &str) -> Result<Value> {
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

pub(super) fn import_antigravity_mcp(source: &str) -> Result<Value> {
    let native: Value = serde_json::from_str(source)?;
    let Some(servers) = native.get("mcpServers").and_then(Value::as_object) else {
        return Ok(json!({ "mcpServers": {} }));
    };
    let mut out = serde_json::Map::new();
    for (name, server) in servers {
        let Some(server_obj) = server.as_object() else {
            continue;
        };
        let mut cfg = serde_json::Map::new();
        if let Some(url) = server_obj
            .get("serverUrl")
            .or_else(|| server_obj.get("url"))
            .cloned()
        {
            cfg.insert("url".into(), url);
        } else {
            copy_json_key(server_obj, &mut cfg, "command");
            copy_json_key(server_obj, &mut cfg, "args");
            copy_json_key(server_obj, &mut cfg, "env");
            copy_json_key(server_obj, &mut cfg, "cwd");
        }
        for key in [
            "headers",
            "authProviderType",
            "oauth",
            "disabled",
            "disabledTools",
        ] {
            copy_json_key(server_obj, &mut cfg, key);
        }
        out.insert(name.clone(), Value::Object(cfg));
    }
    Ok(json!({ "mcpServers": out }))
}

pub(super) fn import_opencode_mcp(source: &str) -> Result<Value> {
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
            copy_json_key(server_obj, &mut cfg, "oauth");
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
            copy_json_key(server_obj, &mut cfg, "cwd");
        }
        copy_json_key(server_obj, &mut cfg, "enabled");
        copy_json_key(server_obj, &mut cfg, "timeout");
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

pub(super) fn import_codex_mcp(source: &str) -> Result<Value> {
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
