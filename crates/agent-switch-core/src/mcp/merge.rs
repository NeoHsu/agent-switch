//! Merge adapters for tool-native MCP files.

use std::io;

use super::convert::{
    convert_antigravity_mcp, convert_copilot_mcp, convert_opencode_mcp, render_codex_mcp_block,
    replace_marker_block,
};
use super::*;

pub(super) fn merge_opencode(canonical_mcp: &Path, target: &Path, check: bool) -> Result<bool> {
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
    let text = format!("{}\n", serde_json::to_string_pretty(&target_json)?);
    if target.exists() && read_existing_text(target)? == text {
        return Ok(false);
    }
    if !check {
        write_if_changed(target, &text)?;
    }
    Ok(true)
}

pub(super) fn merge_codex(canonical_mcp: &Path, target: &Path, check: bool) -> Result<bool> {
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

pub(super) fn merge_copilot(canonical_mcp: &Path, target: &Path, check: bool) -> Result<bool> {
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

pub(super) fn merge_antigravity(canonical_mcp: &Path, target: &Path, check: bool) -> Result<bool> {
    if !canonical_mcp.exists() {
        return Ok(false);
    }
    let canonical: Value = serde_json::from_str(&read_text(canonical_mcp)?)?;
    let converted = convert_antigravity_mcp(&canonical);
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
    obj.insert(
        "mcpServers".into(),
        converted
            .get("mcpServers")
            .cloned()
            .unwrap_or_else(|| json!({})),
    );
    let text = format!("{}\n", serde_json::to_string_pretty(&target_json)?);
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
