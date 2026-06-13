//! Core library for synchronizing canonical `.agents/` files with native agent tool formats.

pub mod config;
pub mod diagnostics;
pub mod formats;
pub mod fs;
pub mod init;
pub mod manifest;
pub mod mcp;
pub mod setup;
pub mod sync;
pub mod tool;
pub mod validator;

pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Errors that map to a specific process exit code. Anything else surfaced
/// through `anyhow` is treated as an I/O failure by the CLI.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    Unsupported(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Ok = 0,
    Drift = 1,
    Config = 2,
    Io = 3,
    Unsupported = 4,
}

impl ExitCode {
    pub fn code(self) -> i32 {
        self as i32
    }
}

#[derive(Debug, Default)]
pub struct CommandOutput {
    pub lines: Vec<String>,
    pub diagnostics: Vec<String>,
    pub exit: Option<ExitCode>,
}

impl CommandOutput {
    pub fn push(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    pub fn diagnostic(&mut self, line: impl Into<String>) {
        self.diagnostics.push(line.into());
    }

    pub fn exit(&self) -> ExitCode {
        self.exit.unwrap_or(ExitCode::Ok)
    }
}
