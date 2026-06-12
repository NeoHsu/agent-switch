use anyhow::Result;
use noyalib::{Mapping, Value};
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
        out[key.as_str()] = yaml_to_toml(val);
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
    // toml_edit::DocumentMut::iter() yields (&str, &Item)
    for (key, item) in doc.iter() {
        if matches!(key, "name" | "description" | "developer_instructions") {
            continue;
        }
        codex.insert(key, toml_to_yaml(item));
    }
    if !codex.is_empty() {
        fm.insert("codex", Value::Mapping(codex));
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
            // as_i64() returns Some only for whole numbers; as_f64() always succeeds.
            if let Some(i) = v.as_i64() {
                value(i)
            } else {
                value(v.as_f64())
            }
        }
        Value::String(v) => value(v),
        Value::Sequence(seq) => {
            let arr = seq.into_iter().filter_map(|v| match v {
                Value::String(s) => Some(toml_edit::Value::from(s)),
                Value::Bool(b) => Some(toml_edit::Value::from(b)),
                Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        Some(toml_edit::Value::from(i))
                    } else {
                        Some(toml_edit::Value::from(n.as_f64()))
                    }
                }
                _ => None,
            });
            value(toml_edit::Array::from_iter(arr))
        }
        Value::Mapping(map) => {
            let mut table = Table::new();
            for (key, val) in map {
                table[key.as_str()] = yaml_to_toml(val);
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
        } else if let Some(f) = v.as_float() {
            Value::Number(f.into())
        } else if let Some(arr) = v.as_array() {
            Value::Sequence(
                arr.iter()
                    .filter_map(|v| {
                        if let Some(s) = v.as_str() {
                            Some(Value::String(s.to_string()))
                        } else if let Some(b) = v.as_bool() {
                            Some(Value::Bool(b))
                        } else if let Some(i) = v.as_integer() {
                            Some(Value::Number(i.into()))
                        } else {
                            v.as_float().map(|f| Value::Number(f.into()))
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
        other => noyalib::to_string(&other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}
