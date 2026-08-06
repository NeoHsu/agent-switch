//! Stable machine-readable output helpers shared by CLI commands.

use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::json;

/// Version of the machine-readable CLI response contract.
pub const SCHEMA_VERSION: u32 = 1;

/// Render an object response with an additive, stable schema version field.
///
/// Human-readable output remains owned by each command, while JSON consumers
/// can rely on `schemaVersion` before interpreting command-specific fields.
pub fn render_json<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    let mut value = serde_json::to_value(value)?;
    let Some(object) = value.as_object_mut() else {
        bail!("machine-readable CLI output must be a JSON object");
    };
    object
        .entry("schemaVersion")
        .or_insert_with(|| json!(SCHEMA_VERSION));
    Ok(serde_json::to_string_pretty(&value)?)
}

/// Render a structured error for commands that requested JSON output.
pub fn render_error(kind: &str, message: &str, exit_code: i32) -> Result<String> {
    render_json(&json!({
        "error": {
            "kind": kind,
            "message": message,
            "exit_code": exit_code,
        }
    }))
}
