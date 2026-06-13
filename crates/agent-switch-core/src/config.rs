use std::{
    collections::{BTreeMap, BTreeSet},
    env, io,
    path::{Component, Path, PathBuf},
    str::FromStr,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

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

const DEFAULT_MERGE: &[(&str, &str, MergeFormat)] = &[
    ("opencode-config", "opencode.json", MergeFormat::Opencode),
    ("codex-config", ".codex/config.toml", MergeFormat::Codex),
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

    validate_repo_path("agents_dir", &cfg.agents_dir)?;
    validate_repo_path("manifest", &cfg.manifest)?;

    let mut generate_outputs = BTreeSet::new();
    for (id, spec) in &cfg.generate {
        validate_id("generate spec", id)?;
        validate_repo_path(&format!("generate spec '{id}' from"), &spec.from)?;
        validate_repo_path(&format!("generate spec '{id}' to"), &spec.to)?;
        validate_tool_selection(
            &format!("generate spec '{id}'"),
            spec.tool,
            spec.tools.as_deref(),
        )?;
        if let Some(suffix) = &spec.suffix {
            validate_suffix(&format!("generate spec '{id}' suffix"), suffix)?;
        }
        let output = repo_path(&spec.to);
        if !generate_outputs.insert(output.clone()) {
            return Err(Error::Config(format!(
                "generate spec '{id}': duplicate output directory: {output}"
            ))
            .into());
        }
    }

    for (link, spec) in &cfg.symlinks {
        let link_path = Path::new(link);
        validate_repo_path(&format!("symlink '{link}' path"), link_path)?;
        validate_repo_path(&format!("symlink '{link}' target"), spec.target())?;
        if repo_path(link_path) == repo_path(spec.target()) {
            return Err(Error::Config(format!(
                "symlink '{link}': link path and target must be different"
            ))
            .into());
        }
        if let SymlinkSpec::Detailed(detail) = spec {
            validate_tool_selection(
                &format!("symlink '{link}'"),
                detail.tool,
                detail.tools.as_deref(),
            )?;
        }
    }

    for (id, spec) in &cfg.merge {
        validate_id("merge spec", id)?;
        validate_repo_path(&format!("merge spec '{id}' to"), &spec.to)?;
        validate_tool_selection(
            &format!("merge spec '{id}'"),
            spec.tool,
            spec.tools.as_deref(),
        )?;
    }
    Ok(())
}

fn validate_id(kind: &str, id: &str) -> Result<()> {
    if id.trim().is_empty() {
        return Err(Error::Config(format!("{kind} id cannot be empty")).into());
    }
    Ok(())
}

fn validate_repo_path(label: &str, path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Err(Error::Config(format!("{label}: path cannot be empty")).into());
    }
    if path.is_absolute() {
        return Err(Error::Config(format!(
            "{label}: path must be repository-relative: {}",
            path.display()
        ))
        .into());
    }
    let raw = path.to_string_lossy();
    if raw.contains('\\') {
        return Err(Error::Config(format!(
            "{label}: use forward slashes for portable paths: {}",
            path.display()
        ))
        .into());
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {
                return Err(Error::Config(format!(
                    "{label}: path cannot contain `.` components: {}",
                    path.display()
                ))
                .into());
            }
            Component::ParentDir => {
                return Err(Error::Config(format!(
                    "{label}: path cannot contain `..` components: {}",
                    path.display()
                ))
                .into());
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(Error::Config(format!(
                    "{label}: path must be repository-relative: {}",
                    path.display()
                ))
                .into());
            }
        }
    }
    Ok(())
}

fn validate_suffix(label: &str, suffix: &str) -> Result<()> {
    if suffix.contains('/') || suffix.contains('\\') {
        return Err(Error::Config(format!(
            "{label}: suffix must be a file-name suffix, not a path: {suffix}"
        ))
        .into());
    }
    Ok(())
}

fn validate_tool_selection(label: &str, tool: Option<Tool>, tools: Option<&[Tool]>) -> Result<()> {
    if tool.is_some() && tools.is_some() {
        return Err(
            Error::Config(format!("{label}: use either `tool` or `tools`, not both")).into(),
        );
    }
    if let Some(tools) = tools {
        if tools.is_empty() {
            return Err(
                Error::Config(format!("{label}: `tools` must contain at least one tool")).into(),
            );
        }
        let mut seen = BTreeSet::new();
        for tool in tools {
            if !seen.insert(*tool) {
                return Err(
                    Error::Config(format!("{label}: duplicate tool `{tool}` in `tools`")).into(),
                );
            }
        }
    }
    Ok(())
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
