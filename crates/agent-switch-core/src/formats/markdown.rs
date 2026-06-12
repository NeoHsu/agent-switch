use std::borrow::Cow;

use anyhow::Result;
use noyalib::{Mapping, Value};

#[derive(Debug, Clone)]
pub struct MarkdownDoc {
    pub frontmatter: Mapping,
    pub body: String,
}

pub fn parse(input: &str) -> Result<MarkdownDoc> {
    // Normalize CRLF so frontmatter fences still parse on files checked out
    // with Windows line endings.
    let text: Cow<'_, str> = if input.contains("\r\n") {
        Cow::Owned(input.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(input)
    };
    let Some(rest) = text.strip_prefix("---\n") else {
        return Ok(MarkdownDoc {
            frontmatter: Mapping::new(),
            body: text.into_owned(),
        });
    };
    let Some((yaml, body)) = split_closing_fence(rest) else {
        return Ok(MarkdownDoc {
            frontmatter: Mapping::new(),
            body: text.into_owned(),
        });
    };
    let frontmatter = if yaml.trim().is_empty() {
        Mapping::new()
    } else {
        noyalib::from_str::<Mapping>(yaml)?
    };
    Ok(MarkdownDoc {
        frontmatter,
        body: body.to_string(),
    })
}

/// Splits the text after the opening fence at the first `---` that forms a
/// complete line, returning the YAML part and the body after the fence.
fn split_closing_fence(rest: &str) -> Option<(&str, &str)> {
    if let Some(body) = rest.strip_prefix("---\n") {
        return Some(("", body));
    }
    if rest == "---" {
        return Some(("", ""));
    }
    let mut from = 0;
    while let Some(pos) = rest[from..].find("\n---") {
        let start = from + pos;
        let after = &rest[start + 4..];
        if after.is_empty() {
            return Some((&rest[..start + 1], ""));
        }
        if let Some(body) = after.strip_prefix('\n') {
            return Some((&rest[..start + 1], body));
        }
        from = start + 4;
    }
    None
}

pub fn render(frontmatter: Mapping, body: &str) -> Result<String> {
    if frontmatter.is_empty() {
        return Ok(body.to_string());
    }
    let mut yaml = noyalib::to_string(&frontmatter)?;
    // Ensure the closing `---` fence appears on its own line.
    if !yaml.ends_with('\n') {
        yaml.push('\n');
    }
    Ok(format!(
        "---\n{}---\n{}",
        yaml,
        body.trim_start_matches('\n')
    ))
}

pub fn str_value(map: &Mapping, key: &str) -> Option<String> {
    map.get(key).and_then(Value::as_str).map(ToOwned::to_owned)
}

pub fn mapping_value(map: &Mapping, key: &str) -> Mapping {
    map.get(key)
        .and_then(Value::as_mapping)
        .cloned()
        .unwrap_or_default()
}

pub fn set_string(map: &mut Mapping, key: &str, value: impl Into<String>) {
    map.insert(key, Value::String(value.into()));
}

pub fn merge_mapping(map: &mut Mapping, other: Mapping) {
    for (key, value) in other {
        map.insert(key, value);
    }
}

pub fn namespace_from_extra(base: &Mapping, exclude: &[&str]) -> Mapping {
    let mut out = Mapping::new();
    for (key, value) in base {
        if !exclude.contains(&key.as_str()) {
            out.insert(key.clone(), value.clone());
        }
    }
    out
}

pub fn base_with_namespace(source: &Mapping, namespace: &str, include: &[&str]) -> Mapping {
    let mut out = Mapping::new();
    for key in include {
        if let Some(value) = source.get(key) {
            out.insert(*key, value.clone());
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
        if let Some(value) = generated.get(key) {
            out.insert(*key, value.clone());
        }
    }
    let ns = namespace_from_extra(generated, exclude);
    if !ns.is_empty() {
        out.insert(tool, Value::Mapping(ns));
    }
    out
}

pub fn paths_to_apply_to(frontmatter: &Mapping) -> String {
    let Some(paths) = frontmatter.get("paths") else {
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
        frontmatter.insert("paths", Value::Sequence(values));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_crlf_frontmatter() {
        let doc = parse("---\r\nname: a\r\n---\r\nbody\r\n").unwrap();
        assert_eq!(str_value(&doc.frontmatter, "name").as_deref(), Some("a"));
        assert_eq!(doc.body, "body\n");
    }

    #[test]
    fn missing_closing_fence_is_treated_as_body() {
        let input = "---\nname: a\nbody without closing fence\n";
        let doc = parse(input).unwrap();
        assert!(doc.frontmatter.is_empty());
        assert_eq!(doc.body, input);
    }

    #[test]
    fn empty_frontmatter_block_parses() {
        let doc = parse("---\n---\nbody\n").unwrap();
        assert!(doc.frontmatter.is_empty());
        assert_eq!(doc.body, "body\n");
    }

    #[test]
    fn closing_fence_at_end_of_input() {
        let doc = parse("---\nname: a\n---").unwrap();
        assert_eq!(str_value(&doc.frontmatter, "name").as_deref(), Some("a"));
        assert_eq!(doc.body, "");
    }

    #[test]
    fn fence_must_be_a_complete_line() {
        let doc = parse("---\nname: a\n---\ndashes\n----\nmore\n").unwrap();
        assert_eq!(str_value(&doc.frontmatter, "name").as_deref(), Some("a"));
        assert_eq!(doc.body, "dashes\n----\nmore\n");
    }

    #[test]
    fn no_frontmatter_passthrough() {
        let doc = parse("plain body\n").unwrap();
        assert!(doc.frontmatter.is_empty());
        assert_eq!(doc.body, "plain body\n");
    }

    #[test]
    fn render_round_trips() {
        let doc = parse("---\nname: a\ndescription: b\n---\nbody\n").unwrap();
        let rendered = render(doc.frontmatter, &doc.body).unwrap();
        assert_eq!(rendered, "---\nname: a\ndescription: b\n---\nbody\n");
    }
}
