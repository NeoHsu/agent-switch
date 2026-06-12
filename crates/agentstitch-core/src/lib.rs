pub mod config;
pub mod diagnostics;
pub mod formats;
pub mod fs;
pub mod init;
pub mod manifest;
pub mod mcp;
pub mod setup;
pub mod sync;

pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

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
    pub exit: Option<ExitCode>,
}

impl CommandOutput {
    pub fn push(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    pub fn exit(&self) -> ExitCode {
        self.exit.unwrap_or(ExitCode::Ok)
    }
}
