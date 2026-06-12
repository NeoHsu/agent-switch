use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{
    fs::read_text,
    tool::{self, Format, MergeFormat, Tool},
    Error,
};

pub const CONFIG_FILE: &str = ".agent-switch.yaml";
pub const LEGACY_CONFIG_FILE: &str = ".agentstitch.yaml";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub version: u32,
    #[serde(default = "default_agents_dir")]
    pub agents_dir: PathBuf,
    #[serde(default = "default_manifest")]
    pub manifest: PathBuf,
    #[serde(default)]
    pub symlinks: BTreeMap<String, SymlinkSpec>,
    #[serde(default)]
    pub generate: BTreeMap<String, GenerateSpec>,
    #[serde(default)]
    pub merge: BTreeMap<String, MergeSpec>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GenerateSpec {
    pub from: PathBuf,
    pub to: PathBuf,
    pub format: Format,
    #[serde(default)]
    pub suffix: Option<String>,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default)]
    pub tool: Option<Tool>,
    #[serde(default)]
    pub tools: Option<Vec<Tool>>,
}

impl GenerateSpec {
    pub fn effective_tools(&self) -> Vec<Tool> {
        explicit_tools(self.tool, self.tools.as_deref()).unwrap_or_else(|| vec![self.format.tool()])
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum SymlinkSpec {
    Target(PathBuf),
    Detailed(SymlinkDetail),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SymlinkDetail {
    #[serde(alias = "target")]
    pub to: PathBuf,
    #[serde(default)]
    pub tool: Option<Tool>,
    #[serde(default)]
    pub tools: Option<Vec<Tool>>,
}

impl SymlinkSpec {
    pub fn target(&self) -> &Path {
        match self {
            Self::Target(target) => target,
            Self::Detailed(detail) => &detail.to,
        }
    }

    pub fn target_config(&self) -> String {
        self.target().to_string_lossy().replace('\\', "/")
    }

    pub fn effective_tools(&self, link: &str) -> Vec<Tool> {
        let explicit = match self {
            Self::Target(_) => None,
            Self::Detailed(detail) => explicit_tools(detail.tool, detail.tools.as_deref()),
        };
        explicit.unwrap_or_else(|| tool::tools_for_link(link, &self.target_config()).to_vec())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MergeSpec {
    pub to: PathBuf,
    /// Explicit merge format. When absent, format is inferred from the spec id
    /// and `to` path for backward compatibility with existing configs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<MergeFormat>,
    #[serde(default)]
    pub tool: Option<Tool>,
    #[serde(default)]
    pub tools: Option<Vec<Tool>>,
}

impl MergeSpec {
    /// Resolves the merge format: explicit field takes precedence, then falls
    /// back to heuristic inference from the spec id and `to` path.
    pub fn resolve_format(&self, id: &str) -> Option<MergeFormat> {
        self.format.or_else(|| {
            if id.starts_with("codex-") || self.to.starts_with(".codex") {
                Some(MergeFormat::Codex)
            } else if id.starts_with("opencode-") || self.to.as_path() == Path::new("opencode.json")
            {
                Some(MergeFormat::Opencode)
            } else {
                None
            }
        })
    }

    pub fn effective_tools(&self, id: &str) -> Vec<Tool> {
        explicit_tools(self.tool, self.tools.as_deref()).unwrap_or_else(|| {
            self.resolve_format(id)
                .map(MergeFormat::tool)
                .into_iter()
                .collect()
        })
    }
}

fn explicit_tools(tool: Option<Tool>, tools: Option<&[Tool]>) -> Option<Vec<Tool>> {
    match tools {
        Some(tools) => Some(tools.to_vec()),
        None => tool.map(|tool| vec![tool]),
    }
}

fn default_agents_dir() -> PathBuf {
    PathBuf::from(".agents")
}

fn default_manifest() -> PathBuf {
    PathBuf::from(".agents/.sync-manifest.json")
}

const DEFAULT_SYMLINKS: &[(&str, &str)] = &[
    (".claude/skills", ".agents/skills"),
    (".claude/agents", ".agents/agents"),
    (".claude/commands", ".agents/commands"),
    (".claude/rules", ".agents/rules"),
    (".opencode/commands", ".agents/commands"),
    (".agent/rules", ".agents/rules"),
    (".agent/workflows", ".agents/commands"),
    (".agent/skills", ".agents/skills"),
    (".mcp.json", ".agents/mcp.json"),
    (".copilot/mcp-config.json", ".agents/mcp.json"),
    (".pi/mcp.json", ".agents/mcp.json"),
    ("CLAUDE.md", "AGENTS.md"),
];

const DEFAULT_GENERATE: &[(&str, &str, &str, Format, &str, bool)] = &[
    (
        "copilot-agents",
        ".agents/agents",
        ".github/agents",
        Format::CopilotAgent,
        ".agent.md",
        false,
    ),
    (
        "copilot-prompts",
        ".agents/commands",
        ".github/prompts",
        Format::CopilotPrompt,
        ".prompt.md",
        false,
    ),
    (
        "copilot-instructions",
        ".agents/rules",
        ".github/instructions",
        Format::CopilotInstructions,
        ".instructions.md",
        true,
    ),
    (
        "opencode-agents",
        ".agents/agents",
        ".opencode/agents",
        Format::OpencodeAgent,
        ".md",
        false,
    ),
    (
        "codex-agents",
        ".agents/agents",
        ".codex/agents",
        Format::CodexAgent,
        ".toml",
        false,
    ),
];

const DEFAULT_MERGE: &[(&str, &str)] = &[
    ("opencode-config", "opencode.json"),
    ("codex-config", ".codex/config.toml"),
];

impl Default for Config {
    fn default() -> Self {
        let symlinks = DEFAULT_SYMLINKS
            .iter()
            .map(|(link, target)| (link.to_string(), SymlinkSpec::Target((*target).into())))
            .collect();
        let generate = DEFAULT_GENERATE
            .iter()
            .map(|&(id, from, to, format, suffix, recursive)| {
                (
                    id.to_string(),
                    GenerateSpec {
                        from: from.into(),
                        to: to.into(),
                        format,
                        suffix: Some(suffix.to_string()),
                        recursive,
                        tool: None,
                        tools: None,
                    },
                )
            })
            .collect();
        let merge = DEFAULT_MERGE
            .iter()
            .map(|&(id, to)| {
                (
                    id.to_string(),
                    MergeSpec {
                        to: to.into(),
                        format: None,
                        tool: None,
                        tools: None,
                    },
                )
            })
            .collect();

        Self {
            version: 1,
            agents_dir: default_agents_dir(),
            manifest: default_manifest(),
            symlinks,
            generate,
            merge,
        }
    }
}

pub fn load_config(root: &Path, explicit: Option<&Path>) -> Result<(Config, PathBuf)> {
    let path = if let Some(path) = explicit {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        }
    } else if root.join(CONFIG_FILE).exists() {
        root.join(CONFIG_FILE)
    } else if root.join(LEGACY_CONFIG_FILE).exists() {
        root.join(LEGACY_CONFIG_FILE)
    } else {
        root.join(CONFIG_FILE)
    };
    let content = read_text(&path)
        .map_err(|err| Error::Config(format!("failed to read config {}: {err}", path.display())))?;
    let cfg: Config = noyalib::from_str(&content)
        .map_err(|err| Error::Config(format!("invalid config {}: {err}", path.display())))?;
    validate_config(&cfg)?;
    Ok((cfg, path))
}

pub fn write_default_config(path: &Path, force: bool) -> Result<bool> {
    if path.exists() && !force {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = noyalib::to_string(&Config::default())?;
    fs::write(path, text)?;
    Ok(true)
}

pub fn find_root(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(root) = explicit {
        return Ok(root.canonicalize().unwrap_or_else(|_| root.to_path_buf()));
    }
    let mut dir = env::current_dir()?;
    loop {
        if dir.join(CONFIG_FILE).exists()
            || dir.join(LEGACY_CONFIG_FILE).exists()
            || dir.join(".agents").exists()
            || dir.join(".git").exists()
        {
            return Ok(dir);
        }
        if !dir.pop() {
            return env::current_dir().map_err(Into::into);
        }
    }
}

pub fn parse_tools(value: &str) -> Result<Vec<Tool>> {
    let tools = value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(Tool::from_str)
        .collect::<Result<Vec<_>, _>>()?;
    if tools.is_empty() {
        return Err(Error::Config("--tool requires a comma-separated tool list".into()).into());
    }
    Ok(tools)
}

pub fn validate_config(cfg: &Config) -> Result<()> {
    if cfg.version != 1 {
        return Err(
            Error::Unsupported(format!("unsupported config version: {}", cfg.version)).into(),
        );
    }
    for (id, spec) in &cfg.merge {
        if spec.resolve_format(id).is_none() {
            return Err(Error::Config(format!(
                "merge spec '{id}': cannot determine format; set an explicit `format:` field (supported: opencode, codex)"
            ))
            .into());
        }
    }
    Ok(())
}

pub fn generate_selected(spec: &GenerateSpec, filter: Option<&[Tool]>) -> bool {
    selected(&spec.effective_tools(), filter)
}

pub fn merge_selected(id: &str, spec: &MergeSpec, filter: Option<&[Tool]>) -> bool {
    selected(&spec.effective_tools(id), filter)
}

pub fn symlink_selected(link: &str, spec: &SymlinkSpec, filter: Option<&[Tool]>) -> bool {
    let tools = spec.effective_tools(link);
    if tools.is_empty() {
        return true;
    }
    selected(&tools, filter)
}

fn selected(mapping_tools: &[Tool], filter: Option<&[Tool]>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    mapping_tools.iter().any(|tool| filter.contains(tool))
}
