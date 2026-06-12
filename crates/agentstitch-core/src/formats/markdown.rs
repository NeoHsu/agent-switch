use std::collections::BTreeMap;

use anyhow::Result;
use serde_yaml::{Mapping, Value};

#[derive(Debug, Clone)]
pub struct MarkdownDoc {
    pub frontmatter: Mapping,
    pub body: String,
}

pub fn parse(input: &str) -> Result<MarkdownDoc> {
    if !input.starts_with("---\n") {
        return Ok(MarkdownDoc {
            frontmatter: Mapping::new(),
            body: input.to_string(),
        });
    }
    let rest = &input[4..];
    if let Some(end) = rest.find("\n---") {
        let yaml = &rest[..end];
        let after = &rest[end + 4..];
        let body = after.strip_prefix('\n').unwrap_or(after).to_string();
        let frontmatter = serde_yaml::from_str::<Mapping>(yaml)?;
        Ok(MarkdownDoc { frontmatter, body })
    } else {
        Ok(MarkdownDoc {
            frontmatter: Mapping::new(),
            body: input.to_string(),
        })
    }
}

pub fn render(frontmatter: Mapping, body: &str) -> Result<String> {
    if frontmatter.is_empty() {
        return Ok(body.to_string());
    }
    let yaml = serde_yaml::to_string(&frontmatter)?;
    Ok(format!(
        "---\n{}---\n{}",
        yaml,
        body.trim_start_matches('\n')
    ))
}

pub fn str_value(map: &Mapping, key: &str) -> Option<String> {
    map.get(Value::String(key.into()))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

pub fn mapping_value(map: &Mapping, key: &str) -> Mapping {
    map.get(Value::String(key.into()))
        .and_then(Value::as_mapping)
        .cloned()
        .unwrap_or_default()
}

pub fn set_string(map: &mut Mapping, key: &str, value: impl Into<String>) {
    map.insert(Value::String(key.into()), Value::String(value.into()));
}

pub fn merge_mapping(map: &mut Mapping, other: Mapping) {
    for (key, value) in other {
        map.insert(key, value);
    }
}

pub fn namespace_from_extra(base: &Mapping, exclude: &[&str]) -> Mapping {
    let mut out = Mapping::new();
    for (key, value) in base {
        let Some(key_str) = key.as_str() else {
            continue;
        };
        if !exclude.contains(&key_str) {
            out.insert(key.clone(), value.clone());
        }
    }
    out
}

pub fn base_with_namespace(source: &Mapping, namespace: &str, include: &[&str]) -> Mapping {
    let mut out = Mapping::new();
    for key in include {
        if let Some(value) = source.get(Value::String((*key).into())) {
            out.insert(Value::String((*key).into()), value.clone());
        }
    }
    let ns = mapping_value(source, namespace);
    merge_mapping(&mut out, ns);
    out
}

pub fn canonical_with_tool_ns(
    tool: &str,
    generated: &Mapping,
    base_keys: &[&str],
    exclude: &[&str],
) -> Mapping {
    let mut out = Mapping::new();
    for key in base_keys {
        if let Some(value) = generated.get(Value::String((*key).into())) {
            out.insert(Value::String((*key).into()), value.clone());
        }
    }
    let ns = namespace_from_extra(generated, exclude);
    if !ns.is_empty() {
        out.insert(Value::String(tool.into()), Value::Mapping(ns));
    }
    out
}

pub fn paths_to_apply_to(frontmatter: &Mapping) -> String {
    let Some(paths) = frontmatter.get(Value::String("paths".into())) else {
        return "**".into();
    };
    match paths {
        Value::Sequence(seq) => {
            let values = seq.iter().filter_map(Value::as_str).collect::<Vec<_>>();
            if values.is_empty() {
                "**".into()
            } else {
                values.join(",")
            }
        }
        Value::String(s) if !s.is_empty() => s.clone(),
        _ => "**".into(),
    }
}

pub fn apply_to_to_paths(apply_to: Option<String>, frontmatter: &mut Mapping) {
    let Some(apply_to) = apply_to else {
        return;
    };
    if apply_to == "**" {
        return;
    }
    let values = apply_to
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| Value::String(s.into()))
        .collect::<Vec<_>>();
    if !values.is_empty() {
        frontmatter.insert(Value::String("paths".into()), Value::Sequence(values));
    }
}

pub fn mapping_from_pairs(pairs: BTreeMap<String, Value>) -> Mapping {
    pairs
        .into_iter()
        .map(|(k, v)| (Value::String(k), v))
        .collect()
}
