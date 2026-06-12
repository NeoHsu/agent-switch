use anyhow::Result;
use serde_yaml::{Mapping, Value};
use toml_edit::{value, DocumentMut, Item, Table};

use super::markdown::{self, render, set_string};

pub fn export_agent(source: &str) -> Result<String> {
    let doc = markdown::parse(source)?;
    let mut out = DocumentMut::new();
    if let Some(name) = markdown::str_value(&doc.frontmatter, "name") {
        out["name"] = value(name);
    }
    if let Some(description) = markdown::str_value(&doc.frontmatter, "description") {
        out["description"] = value(description);
    }
    for (key, val) in markdown::mapping_value(&doc.frontmatter, "codex") {
        let Some(key) = key.as_str() else {
            continue;
        };
        out[key] = yaml_to_toml(val);
    }
    out["developer_instructions"] = value(doc.body.trim_end().to_string());
    Ok(out.to_string())
}

pub fn import_agent(source: &str) -> Result<String> {
    let doc = source.parse::<DocumentMut>()?;
    let mut fm = Mapping::new();
    if let Some(name) = doc.get("name").and_then(|v| v.as_str()) {
        set_string(&mut fm, "name", name);
    }
    if let Some(description) = doc.get("description").and_then(|v| v.as_str()) {
        set_string(&mut fm, "description", description);
    }
    let mut codex = Mapping::new();
    for (key, item) in doc.iter() {
        if matches!(key, "name" | "description" | "developer_instructions") {
            continue;
        }
        codex.insert(Value::String(key.to_string()), toml_to_yaml(item));
    }
    if !codex.is_empty() {
        fm.insert(Value::String("codex".into()), Value::Mapping(codex));
    }
    let body = doc
        .get("developer_instructions")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    render(fm, body)
}

fn yaml_to_toml(yaml_value: Value) -> Item {
    match yaml_value {
        Value::Bool(v) => value(v),
        Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                value(i)
            } else if let Some(f) = v.as_f64() {
                value(f)
            } else {
                value(v.to_string())
            }
        }
        Value::String(v) => value(v),
        Value::Sequence(seq) => {
            let arr = seq.into_iter().filter_map(|v| match v {
                Value::String(s) => Some(toml_edit::Value::from(s)),
                Value::Bool(b) => Some(toml_edit::Value::from(b)),
                Value::Number(n) => n.as_i64().map(toml_edit::Value::from),
                _ => None,
            });
            value(toml_edit::Array::from_iter(arr))
        }
        Value::Mapping(map) => {
            let mut table = Table::new();
            for (key, val) in map {
                if let Some(key) = key.as_str() {
                    table[key] = yaml_to_toml(val);
                }
            }
            Item::Table(table)
        }
        _ => value(value_to_string(yaml_value)),
    }
}

fn toml_to_yaml(item: &Item) -> Value {
    if let Some(v) = item.as_value() {
        if let Some(s) = v.as_str() {
            Value::String(s.to_string())
        } else if let Some(b) = v.as_bool() {
            Value::Bool(b)
        } else if let Some(i) = v.as_integer() {
            Value::Number(i.into())
        } else if let Some(arr) = v.as_array() {
            Value::Sequence(
                arr.iter()
                    .filter_map(|v| {
                        if let Some(s) = v.as_str() {
                            Some(Value::String(s.to_string()))
                        } else {
                            v.as_integer().map(|i| Value::Number(i.into()))
                        }
                    })
                    .collect(),
            )
        } else {
            Value::String(v.to_string())
        }
    } else {
        Value::String(item.to_string())
    }
}

fn value_to_string(value: Value) -> String {
    match value {
        Value::String(s) => s,
        other => serde_yaml::to_string(&other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}
