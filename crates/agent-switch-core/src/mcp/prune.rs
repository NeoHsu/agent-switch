//! Conservative removal of tool-native MCP output.

use super::convert::convert_copilot_mcp;
use super::*;

pub(super) fn prune_opencode(target: &Path, check: bool) -> Result<PruneOutcome> {
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

pub(super) fn prune_antigravity(target: &Path, check: bool) -> Result<PruneOutcome> {
    let Ok(mut existing) = serde_json::from_str::<Value>(&read_text(target)?) else {
        return Ok(PruneOutcome::Unmanaged);
    };
    let Some(obj) = existing.as_object_mut() else {
        return Ok(PruneOutcome::Unmanaged);
    };
    if obj.remove("mcpServers").is_none() {
        return Ok(PruneOutcome::Absent);
    }
    if obj.is_empty() {
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

pub(super) fn prune_codex(target: &Path, check: bool) -> Result<PruneOutcome> {
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

pub(super) fn prune_copilot(
    canonical_mcp: &Path,
    target: &Path,
    check: bool,
) -> Result<PruneOutcome> {
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
