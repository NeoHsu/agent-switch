//! Configuration model, defaults, loading, and selection helpers.

use std::{
    collections::BTreeMap,
    env, io,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use walkdir::{DirEntry, WalkDir};

use crate::{
    Error,
    fs::{atomic_write, read_text, repo_path},
    tool::{self, Format, MergeFormat, Tool},
};

pub const CONFIG_FILE: &str = ".agent-switch.yaml";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub version: u32,
    #[serde(default = "default_agents_dir")]
    pub agents_dir: PathBuf,
    #[serde(default = "default_manifest")]
    pub manifest: PathBuf,
    #[serde(default)]
    pub sync_mode: SyncMode,
    #[serde(default)]
    pub generated_tracking: BTreeMap<String, GeneratedTracking>,
    #[serde(default)]
    pub symlinks: BTreeMap<String, SymlinkSpec>,
    #[serde(default)]
    pub generate: BTreeMap<String, GenerateSpec>,
    #[serde(default)]
    pub merge: BTreeMap<String, MergeSpec>,
}

#[derive(Debug, Clone)]
pub struct ManagedLink {
    pub link: PathBuf,
    pub target: PathBuf,
    pub target_config: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncMode {
    Full,
    #[default]
    CanonicalOnly,
    ExportOnly,
    ImportOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GeneratedTracking {
    Tracked,
    Ignored,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<Tool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    pub to: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<Tool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    pub format: MergeFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<Tool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
}

impl MergeSpec {
    pub fn effective_tools(&self) -> Vec<Tool> {
        explicit_tools(self.tool, self.tools.as_deref()).unwrap_or_else(|| vec![self.format.tool()])
    }
}

fn explicit_tools(tool: Option<Tool>, tools: Option<&[Tool]>) -> Option<Vec<Tool>> {
    match tools {
        Some(tools) => Some(tools.to_vec()),
        None => tool.map(|tool| vec![tool]),
    }
}

fn default_agents_dir() -> PathBuf {
    PathBuf::from(".agent")
}

fn default_manifest() -> PathBuf {
    PathBuf::from(".agent/.sync-manifest.json")
}

const DEFAULT_SYMLINKS: &[(&str, &str)] = &[
    (".claude/skills", "skills"),
    (".claude/agents", "agents"),
    (".claude/commands", "commands"),
    (".claude/rules", "rules"),
    (".opencode/commands", "commands"),
    (".mcp.json", "mcp.json"),
    (".pi/mcp.json", "mcp.json"),
    ("CLAUDE.md", "AGENTS.md"),
];

const DEFAULT_GENERATE: &[(&str, &str, &str, Format, &str, bool)] = &[
    (
        "copilot-agents",
        "agents",
        ".github/agents",
        Format::CopilotAgent,
        ".agent.md",
        false,
    ),
    (
        "copilot-prompts",
        "commands",
        ".github/prompts",
        Format::CopilotPrompt,
        ".prompt.md",
        false,
    ),
    (
        "copilot-instructions",
        "rules",
        ".github/instructions",
        Format::CopilotInstructions,
        ".instructions.md",
        true,
    ),
    (
        "opencode-agents",
        "agents",
        ".opencode/agents",
        Format::OpencodeAgent,
        ".md",
        false,
    ),
    (
        "codex-agents",
        "agents",
        ".codex/agents",
        Format::CodexAgent,
        ".toml",
        false,
    ),
];

const DEFAULT_MERGE: &[(&str, &str, MergeFormat)] = &[
    ("opencode-config", "opencode.json", MergeFormat::Opencode),
    ("codex-config", ".codex/config.toml", MergeFormat::Codex),
    (
        "copilot-mcp-config",
        ".copilot/mcp-config.json",
        MergeFormat::Copilot,
    ),
];

impl Default for Config {
    fn default() -> Self {
        Self::for_agents_dir(default_agents_dir())
    }
}

impl Config {
    pub fn for_agents_dir(agents_dir: PathBuf) -> Self {
        let symlinks = DEFAULT_SYMLINKS
            .iter()
            .map(|(link, target)| {
                let target = if *target == "AGENTS.md" {
                    PathBuf::from(target)
                } else {
                    repo_join(&agents_dir, target)
                };
                (link.to_string(), SymlinkSpec::Target(target))
            })
            .collect();
        let generate = DEFAULT_GENERATE
            .iter()
            .map(|&(id, from, to, format, suffix, recursive)| {
                (
                    id.to_string(),
                    GenerateSpec {
                        from: repo_join(&agents_dir, from),
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
            .map(|&(id, to, format)| {
                (
                    id.to_string(),
                    MergeSpec {
                        to: to.into(),
                        format,
                        tool: None,
                        tools: None,
                    },
                )
            })
            .collect();
        let generated_tracking = [
            ("copilot-agents", GeneratedTracking::Tracked),
            ("copilot-prompts", GeneratedTracking::Tracked),
            ("copilot-instructions", GeneratedTracking::Tracked),
            ("claude", GeneratedTracking::Ignored),
            ("codex-agents", GeneratedTracking::Ignored),
            ("opencode-agents", GeneratedTracking::Ignored),
            ("opencode-config", GeneratedTracking::Ignored),
            ("copilot-mcp-config", GeneratedTracking::Ignored),
            ("codex-config", GeneratedTracking::Ignored),
        ]
        .into_iter()
        .map(|(id, tracking)| (id.to_string(), tracking))
        .collect();

        let manifest = repo_join(&agents_dir, ".sync-manifest.json");

        Self {
            version: 1,
            agents_dir,
            manifest,
            sync_mode: SyncMode::CanonicalOnly,
            generated_tracking,
            symlinks,
            generate,
            merge,
        }
    }
}

fn repo_join(base: &Path, child: &str) -> PathBuf {
    let base = repo_path(base);
    if base.is_empty() {
        PathBuf::from(child)
    } else {
        PathBuf::from(format!("{base}/{child}"))
    }
}

pub fn resolve_config_path(root: &Path, explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        }
    } else {
        root.join(CONFIG_FILE)
    }
}

pub fn load_config(root: &Path, explicit: Option<&Path>) -> Result<(Config, PathBuf)> {
    let path = resolve_config_path(root, explicit);
    let content = read_text(&path).map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            Error::Config("No config file found. Run 'ags init' to create one.".into())
        } else {
            Error::Config(format!("failed to read config {}: {err}", path.display()))
        }
    })?;
    let cfg: Config = noyalib::from_str(&content)
        .map_err(|err| Error::Config(format!("invalid config {}: {err}", path.display())))?;
    validate_config(&cfg)?;
    Ok((cfg, path))
}

pub fn write_config(path: &Path, cfg: &Config, force: bool) -> Result<bool> {
    if path.exists() && !force {
        return Ok(false);
    }
    let text = noyalib::to_string(cfg)?;
    atomic_write(path, text.as_bytes())?;
    Ok(true)
}

pub fn write_default_config(path: &Path, force: bool) -> Result<bool> {
    write_config(path, &Config::default(), force)
}

pub fn find_root(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(root) = explicit {
        return Ok(root.canonicalize().unwrap_or_else(|_| root.to_path_buf()));
    }
    let mut dir = env::current_dir()?;
    loop {
        if dir.join(CONFIG_FILE).exists()
            || dir.join(".agent").exists()
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
    crate::validator::validate_config(cfg)
}

pub fn generate_selected(spec: &GenerateSpec, filter: Option<&[Tool]>) -> bool {
    selected(&spec.effective_tools(), filter)
}

pub fn merge_selected(_id: &str, spec: &MergeSpec, filter: Option<&[Tool]>) -> bool {
    selected(&spec.effective_tools(), filter)
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

pub fn claude_instruction_links(root: &Path) -> Result<Vec<ManagedLink>> {
    let mut links = Vec::new();
    for entry in WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| should_visit_instruction_entry(root, entry))
    {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name() != "AGENTS.md" {
            continue;
        }
        let rel = entry.path().strip_prefix(root)?;
        if rel.components().count() <= 1 {
            continue;
        }
        let mut link = rel.to_path_buf();
        link.set_file_name("CLAUDE.md");
        links.push(ManagedLink {
            link,
            target: rel.to_path_buf(),
            target_config: rel.to_string_lossy().replace('\\', "/"),
        });
    }
    Ok(links)
}

fn should_visit_instruction_entry(root: &Path, entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    if !entry.file_type().is_dir() {
        return true;
    }
    let Ok(rel) = entry.path().strip_prefix(root) else {
        return false;
    };
    let Some(name) = rel
        .components()
        .next_back()
        .and_then(|c| c.as_os_str().to_str())
    else {
        return false;
    };
    !matches!(
        name,
        ".git"
            | ".agent"
            | ".agents"
            | ".claude"
            | ".codex"
            | ".copilot"
            | ".github"
            | ".opencode"
            | ".pi"
            | "node_modules"
            | "target"
    )
}
