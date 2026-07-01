//! Supported tool identifiers and format ownership rules.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum::{EnumIter, IntoEnumIterator};

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, EnumIter)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Claude,
    Codex,
    Copilot,
    Opencode,
    Pi,
    Antigravity,
}

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Codex => "codex",
            Tool::Copilot => "copilot",
            Tool::Opencode => "opencode",
            Tool::Pi => "pi",
            Tool::Antigravity => "antigravity",
        }
    }
}

impl fmt::Display for Tool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Tool {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Tool::iter().find(|tool| tool.name() == s).ok_or_else(|| {
            Error::Config(format!(
                "unknown tool: {s}; supported tools: {}",
                supported_tools()
            ))
        })
    }
}

pub fn supported_tools() -> String {
    Tool::iter().map(Tool::name).collect::<Vec<_>>().join(", ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumIter)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    CopilotAgent,
    CopilotPrompt,
    CopilotInstructions,
    OpencodeAgent,
    CodexAgent,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::CopilotAgent => "copilot-agent",
            Format::CopilotPrompt => "copilot-prompt",
            Format::CopilotInstructions => "copilot-instructions",
            Format::OpencodeAgent => "opencode-agent",
            Format::CodexAgent => "codex-agent",
        }
    }

    pub fn tool(self) -> Tool {
        match self {
            Format::CopilotAgent | Format::CopilotPrompt | Format::CopilotInstructions => {
                Tool::Copilot
            }
            Format::OpencodeAgent => Tool::Opencode,
            Format::CodexAgent => Tool::Codex,
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Format {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Format::iter()
            .find(|fmt| fmt.name() == s)
            .ok_or_else(|| Error::Config(format!("unknown format: {s}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeFormat {
    Opencode,
    Codex,
    Copilot,
}

impl MergeFormat {
    pub fn name(self) -> &'static str {
        match self {
            MergeFormat::Opencode => "opencode",
            MergeFormat::Codex => "codex",
            MergeFormat::Copilot => "copilot",
        }
    }

    pub fn tool(self) -> Tool {
        match self {
            MergeFormat::Opencode => Tool::Opencode,
            MergeFormat::Codex => Tool::Codex,
            MergeFormat::Copilot => Tool::Copilot,
        }
    }
}

impl fmt::Display for MergeFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

// Symlink ownership rules. Exact matches win over prefixes; prefixes are
// checked in order, so `.claude/skills` must stay ahead of `.claude/`.
const LINK_EXACT_RULES: &[(&str, &[Tool])] = &[
    (".mcp.json", &[Tool::Claude]),
    ("CLAUDE.md", &[Tool::Claude]),
];

const LINK_PREFIX_RULES: &[(&str, &[Tool])] = &[
    (".claude/skills", &[Tool::Claude, Tool::Pi]),
    (".claude/", &[Tool::Claude]),
    (".copilot/", &[Tool::Copilot]),
    (".opencode/", &[Tool::Opencode]),
    (".pi/", &[Tool::Pi]),
    (".agent/", &[Tool::Antigravity]),
];

const SKILLS_TARGET_TOOLS: &[Tool] = &[Tool::Claude, Tool::Pi];

pub fn tools_for_link(link: &str, target: &str) -> &'static [Tool] {
    for (path, tools) in LINK_EXACT_RULES {
        if link == *path {
            return tools;
        }
    }
    for (prefix, tools) in LINK_PREFIX_RULES {
        if link.starts_with(prefix) {
            return tools;
        }
    }
    if target.contains(".agent/skills") {
        return SKILLS_TARGET_TOOLS;
    }
    &[]
}
