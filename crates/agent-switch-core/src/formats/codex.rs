use anyhow::Result;
use noyalib::{Mapping, Value};
use toml_edit::{DocumentMut, InlineTable, Item, Table, value};

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
        Value::Mapping(map) => {
            let mut table = Table::new();
            for (key, val) in map {
                table[key.as_str()] = yaml_to_toml(val);
            }
            Item::Table(table)
        }
        other => Item::Value(yaml_to_toml_value(other)),
    }
}

fn yaml_to_toml_value(yaml_value: Value) -> toml_edit::Value {
    match yaml_value {
        Value::Bool(v) => toml_edit::Value::from(v),
        Value::Number(v) => {
            // as_i64() returns Some only for whole numbers; as_f64() always succeeds.
            if let Some(i) = v.as_i64() {
                toml_edit::Value::from(i)
            } else {
                toml_edit::Value::from(v.as_f64())
            }
        }
        Value::String(v) => toml_edit::Value::from(v),
        Value::Sequence(seq) => {
            let arr = seq.into_iter().map(yaml_to_toml_value);
            toml_edit::Value::Array(toml_edit::Array::from_iter(arr))
        }
        Value::Mapping(map) => {
            let mut table = InlineTable::new();
            for (key, val) in map {
                table.insert(key, yaml_to_toml_value(val));
            }
            toml_edit::Value::InlineTable(table)
        }
        other => toml_edit::Value::from(value_to_string(other)),
    }
}

fn toml_to_yaml(item: &Item) -> Value {
    if let Some(v) = item.as_value() {
        return toml_value_to_yaml(v);
    }
    if let Some(table) = item.as_table() {
        return table_to_yaml(table);
    }
    if let Some(array) = item.as_array_of_tables() {
        return Value::Sequence(array.iter().map(table_to_yaml).collect());
    }
    Value::String(item.to_string())
}

fn toml_value_to_yaml(value: &toml_edit::Value) -> Value {
    if let Some(s) = value.as_str() {
        Value::String(s.to_string())
    } else if let Some(b) = value.as_bool() {
        Value::Bool(b)
    } else if let Some(i) = value.as_integer() {
        Value::Number(i.into())
    } else if let Some(f) = value.as_float() {
        Value::Number(f.into())
    } else if let Some(arr) = value.as_array() {
        Value::Sequence(arr.iter().map(toml_value_to_yaml).collect())
    } else if let Some(table) = value.as_inline_table() {
        inline_table_to_yaml(table)
    } else if let Some(datetime) = value.as_datetime() {
        Value::String(datetime.to_string())
    } else {
        Value::String(value.to_string())
    }
}

fn table_to_yaml(table: &Table) -> Value {
    let mut map = Mapping::new();
    for (key, item) in table.iter() {
        map.insert(key, toml_to_yaml(item));
    }
    Value::Mapping(map)
}

fn inline_table_to_yaml(table: &InlineTable) -> Value {
    let mut map = Mapping::new();
    for (key, value) in table.iter() {
        map.insert(key, toml_value_to_yaml(value));
    }
    Value::Mapping(map)
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
