//! Merge helpers for Model Context Protocol configuration files.

use std::{fs, path::Path};

use anyhow::Result;
use serde_json::{Value, json};

use crate::{
    Error,
    fs::{io_error, read_text, write_if_changed},
    tool::MergeFormat,
};

mod convert;
mod import;
mod merge;
mod prune;

use import::{import_antigravity_mcp, import_codex_mcp, import_copilot_mcp, import_opencode_mcp};
use merge::{merge_antigravity, merge_codex, merge_copilot, merge_opencode};
use prune::{prune_antigravity, prune_codex, prune_copilot, prune_opencode};

const CODEX_START: &str = "# >>> agent-switch:mcp >>>";
const CODEX_END: &str = "# <<< agent-switch:mcp <<<";

pub const EMPTY_MCP: &str = "{\n  \"mcpServers\": {}\n}\n";

pub fn merge(
    format: MergeFormat,
    canonical_mcp: &Path,
    target: &Path,
    check: bool,
) -> Result<bool> {
    match format {
        MergeFormat::Opencode => merge_opencode(canonical_mcp, target, check),
        MergeFormat::Codex => merge_codex(canonical_mcp, target, check),
        MergeFormat::Copilot => merge_copilot(canonical_mcp, target, check),
        MergeFormat::Antigravity => merge_antigravity(canonical_mcp, target, check),
    }
}

/// Result of pruning agent-switch managed MCP content from a merge target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneOutcome {
    /// The whole file was agent-switch managed and has been removed.
    Removed,
    /// Managed content was stripped; user-owned content was kept in place.
    Cleaned,
    /// The file exists but is not recognizable as agent-switch output.
    Unmanaged,
    /// Nothing managed was present; no action needed.
    Absent,
}

/// Remove agent-switch managed MCP content from a merge target when the
/// owning tool is no longer selected. Only content this tool can prove it
/// generated is touched; anything else is reported as unmanaged.
pub fn prune(
    format: MergeFormat,
    canonical_mcp: &Path,
    target: &Path,
    check: bool,
) -> Result<PruneOutcome> {
    if !target.exists() {
        return Ok(PruneOutcome::Absent);
    }
    if !target.is_file() {
        return Ok(PruneOutcome::Unmanaged);
    }
    match format {
        MergeFormat::Opencode => prune_opencode(target, check),
        MergeFormat::Codex => prune_codex(target, check),
        MergeFormat::Copilot => prune_copilot(canonical_mcp, target, check),
        MergeFormat::Antigravity => prune_antigravity(target, check),
    }
}

pub fn canonical_mcp_path(root: &Path, agents_dir: &Path) -> std::path::PathBuf {
    root.join(agents_dir).join("mcp.json")
}

pub fn import_native(format: MergeFormat, target: &Path) -> Result<Option<Value>> {
    if !target.exists() {
        return Ok(None);
    }
    let text = read_text(target)?;
    let canonical = match format {
        MergeFormat::Opencode => import_opencode_mcp(&text)?,
        MergeFormat::Codex => import_codex_mcp(&text)?,
        MergeFormat::Copilot => import_copilot_mcp(&text)?,
        MergeFormat::Antigravity => import_antigravity_mcp(&text)?,
    };
    Ok(Some(canonical))
}

#[cfg(test)]
mod tests {
    use super::convert::{
        convert_antigravity_mcp, convert_copilot_mcp, convert_opencode_mcp, render_codex_mcp_block,
        replace_marker_block,
    };
    use super::import::{import_antigravity_mcp, import_opencode_mcp};
    use super::*;

    fn block() -> &'static str {
        "# >>> agent-switch:mcp >>>\n[mcp_servers.demo]\ncommand = \"npx\"\n# <<< agent-switch:mcp <<<\n"
    }

    #[test]
    fn marker_block_replaces_current_markers() {
        let existing = "theme = \"dark\"\n\n# >>> agent-switch:mcp >>>\nold = true\n# <<< agent-switch:mcp <<<\n";
        let next = replace_marker_block(existing, block());

        assert!(next.contains("theme = \"dark\""));
        assert!(next.contains("[mcp_servers.demo]"));
        assert!(!next.contains("old = true"));
        assert!(next.ends_with('\n'));
    }

    #[test]
    fn marker_block_handles_missing_end_marker() {
        let existing = "theme = \"dark\"\n\n# >>> agent-switch:mcp >>>\nold = true\n";
        let next = replace_marker_block(existing, block());

        assert_eq!(
            next,
            "theme = \"dark\"\n\n# >>> agent-switch:mcp >>>\n[mcp_servers.demo]\ncommand = \"npx\"\n# <<< agent-switch:mcp <<<\n"
        );
    }

    #[test]
    fn marker_block_uses_block_for_empty_file() {
        assert_eq!(replace_marker_block("\n\n", block()), block());
    }

    #[test]
    fn codex_mcp_block_renders_command_args_and_env() {
        let canonical = json!({
            "mcpServers": {
                "context7": {
                    "command": "npx",
                    "args": ["-y", "@upstash/context7-mcp"],
                    "env": {"KEY": "${KEY}"}
                }
            }
        });

        let rendered = render_codex_mcp_block(&canonical);

        assert!(rendered.contains("mcp_servers"));
        assert!(rendered.contains("context7"));
        assert!(rendered.contains("command = \"npx\""));
        assert!(rendered.contains("args = [\"-y\", \"@upstash/context7-mcp\"]"));
        assert!(rendered.contains("env = { KEY = \"${KEY}\" }"));
    }

    #[test]
    fn codex_mcp_block_renders_http_servers_and_tool_policy() {
        let canonical = json!({
            "mcpServers": {
                "figma": {
                    "url": "https://mcp.figma.com/mcp",
                    "bearer_token_env_var": "FIGMA_TOKEN",
                    "headers": {"X-Figma-Region": "us-east-1"},
                    "enabled_tools": ["inspect"],
                    "disabled_tools": ["write"],
                    "startup_timeout_sec": 20,
                    "enabled": true
                }
            }
        });

        let rendered = render_codex_mcp_block(&canonical);

        assert!(rendered.contains("[mcp_servers.figma]"));
        assert!(rendered.contains("url = \"https://mcp.figma.com/mcp\""));
        assert!(rendered.contains("bearer_token_env_var = \"FIGMA_TOKEN\""));
        assert!(rendered.contains("http_headers = { X-Figma-Region = \"us-east-1\" }"));
        assert!(rendered.contains("enabled_tools = [\"inspect\"]"));
        assert!(rendered.contains("disabled_tools = [\"write\"]"));
        assert!(rendered.contains("startup_timeout_sec = 20"));
        assert!(rendered.contains("enabled = true"));
    }

    #[test]
    fn opencode_mcp_conversion_preserves_current_native_options() -> Result<()> {
        let canonical = json!({
            "mcpServers": {
                "local": {
                    "command": "node",
                    "args": ["server.js"],
                    "env": {"KEY": "value"},
                    "cwd": "tools",
                    "enabled": false,
                    "timeout": 9000
                },
                "remote": {
                    "url": "https://example.com/mcp",
                    "headers": {"Authorization": "Bearer token"},
                    "oauth": {"clientId": "client"},
                    "enabled": false,
                    "timeout": 7000
                }
            }
        });

        let converted = convert_opencode_mcp(&canonical);
        assert_eq!(converted["local"]["enabled"], false);
        assert_eq!(converted["local"]["cwd"], "tools");
        assert_eq!(converted["local"]["timeout"], 9000);
        assert_eq!(converted["remote"]["oauth"]["clientId"], "client");
        assert_eq!(converted["remote"]["enabled"], false);

        let native = json!({ "mcp": converted });
        let imported = import_opencode_mcp(&serde_json::to_string(&native)?)?;
        assert_eq!(imported["mcpServers"]["local"]["enabled"], false);
        assert_eq!(imported["mcpServers"]["local"]["cwd"], "tools");
        assert_eq!(imported["mcpServers"]["local"]["timeout"], 9000);
        assert_eq!(
            imported["mcpServers"]["remote"]["oauth"]["clientId"],
            "client"
        );
        Ok(())
    }

    #[test]
    fn antigravity_mcp_conversion_normalizes_remote_urls_and_round_trips() -> Result<()> {
        let canonical = json!({
            "mcpServers": {
                "local": {
                    "command": "node",
                    "args": ["server.js"],
                    "env": {"KEY": "value"}
                },
                "remote": {
                    "url": "https://example.com/mcp",
                    "headers": {"Authorization": "Bearer token"},
                    "disabled_tools": ["dangerous"]
                }
            }
        });

        let converted = convert_antigravity_mcp(&canonical);
        assert_eq!(
            converted["mcpServers"]["remote"]["serverUrl"],
            "https://example.com/mcp"
        );
        assert!(converted["mcpServers"]["remote"].get("url").is_none());
        assert_eq!(
            converted["mcpServers"]["remote"]["disabledTools"],
            json!(["dangerous"])
        );
        assert_eq!(converted["mcpServers"]["local"]["command"], "node");

        let imported = import_antigravity_mcp(&serde_json::to_string(&converted)?)?;
        assert_eq!(
            imported["mcpServers"]["remote"]["url"],
            "https://example.com/mcp"
        );
        assert!(imported["mcpServers"]["remote"].get("serverUrl").is_none());
        Ok(())
    }

    #[test]
    fn copilot_mcp_conversion_adds_required_type_and_tools() {
        let canonical = json!({
            "mcpServers": {
                "playwright": {
                    "command": "npx",
                    "args": ["@playwright/mcp@latest"],
                    "env": {"KEY": "${KEY}"}
                },
                "context7": {
                    "type": "http",
                    "url": "https://mcp.context7.com/mcp",
                    "headers": {"CONTEXT7_API_KEY": "${COPILOT_MCP_CONTEXT7_API_KEY}"},
                    "tools": ["resolve-library-id"]
                }
            }
        });

        let converted = convert_copilot_mcp(&canonical);

        assert_eq!(
            converted["mcpServers"]["playwright"]["type"],
            json!("local")
        );
        assert_eq!(converted["mcpServers"]["playwright"]["tools"], json!(["*"]));
        assert_eq!(
            converted["mcpServers"]["playwright"]["args"],
            json!(["@playwright/mcp@latest"])
        );
        assert_eq!(converted["mcpServers"]["context7"]["type"], json!("http"));
        assert_eq!(
            converted["mcpServers"]["context7"]["tools"],
            json!(["resolve-library-id"])
        );
    }
}
