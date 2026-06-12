pub mod codex;
pub mod copilot;
pub mod markdown;
pub mod opencode;

use std::path::Path;

use anyhow::Result;

use crate::tool::Format;

impl Format {
    pub fn export(self, source: &str) -> Result<String> {
        let out = match self {
            Format::CopilotAgent => copilot::export_agent(source),
            Format::CopilotPrompt => copilot::export_prompt(source),
            Format::CopilotInstructions => copilot::export_instructions(source),
            Format::OpencodeAgent => opencode::export_agent(source),
            Format::CodexAgent => codex::export_agent(source),
        }?;
        Ok(ensure_trailing_newline(out))
    }

    pub fn import(self, generated_path: &Path, generated: &str) -> Result<String> {
        let out = match self {
            Format::CopilotAgent => copilot::import_agent(generated),
            Format::CopilotPrompt => copilot::import_prompt(generated),
            Format::CopilotInstructions => copilot::import_instructions(generated),
            Format::OpencodeAgent => opencode::import_agent(generated_path, generated),
            Format::CodexAgent => codex::import_agent(generated),
        }?;
        Ok(ensure_trailing_newline(out))
    }
}

pub fn export(format: Format, source: &str) -> Result<String> {
    format.export(source)
}

pub fn import(format: Format, generated_path: &Path, generated: &str) -> Result<String> {
    format.import(generated_path, generated)
}

fn ensure_trailing_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}
