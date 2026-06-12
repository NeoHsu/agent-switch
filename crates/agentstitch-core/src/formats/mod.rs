pub mod codex;
pub mod copilot;
pub mod markdown;
pub mod opencode;

use std::path::Path;

use anyhow::{anyhow, Result};

pub fn export(format: &str, source_path: &Path, source: &str) -> Result<String> {
    match format {
        "copilot-agent" => copilot::export_agent(source),
        "copilot-prompt" => copilot::export_prompt(source),
        "copilot-instructions" => copilot::export_instructions(source),
        "opencode-agent" => opencode::export_agent(source),
        "codex-agent" => codex::export_agent(source),
        _ => Err(anyhow!("error: unsupported generate format: {format}")),
    }
    .map(|mut out| {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        let _ = source_path;
        out
    })
}

pub fn import(format: &str, generated_path: &Path, generated: &str) -> Result<String> {
    match format {
        "copilot-agent" => copilot::import_agent(generated),
        "copilot-prompt" => copilot::import_prompt(generated),
        "copilot-instructions" => copilot::import_instructions(generated),
        "opencode-agent" => opencode::import_agent(generated_path, generated),
        "codex-agent" => codex::import_agent(generated),
        _ => Err(anyhow!("error: unsupported generate format: {format}")),
    }
    .map(|mut out| {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    })
}
