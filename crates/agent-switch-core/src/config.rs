use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

pub const CONFIG_FILE: &str = ".agent-switch.yaml";
pub const LEGACY_CONFIG_FILE: &str = ".agentstitch.yaml";

pub const SUPPORTED_TOOLS: &[&str] = &[
    "claude",
    "codex",
    "copilot",
    "opencode",
    "pi",
    "antigravity",
];

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub version: u32,
    #[serde(default = "default_agents_dir")]
    pub agents_dir: PathBuf,
    #[serde(default = "default_manifest")]
    pub manifest: PathBuf,
    #[serde(default)]
    pub symlinks: BTreeMap<String, String>,
    #[serde(default)]
    pub generate: BTreeMap<String, GenerateSpec>,
    #[serde(default)]
    pub merge: BTreeMap<String, MergeSpec>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GenerateSpec {
    pub from: PathBuf,
    pub to: PathBuf,
    pub format: String,
    #[serde(default)]
    pub suffix: Option<String>,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MergeSpec {
    pub to: PathBuf,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
}

fn default_agents_dir() -> PathBuf {
    PathBuf::from(".agents")
}

fn default_manifest() -> PathBuf {
    PathBuf::from(".agents/.sync-manifest.json")
}

impl Default for Config {
    fn default() -> Self {
        let mut symlinks = BTreeMap::new();
        symlinks.insert(".claude/skills".into(), ".agents/skills".into());
        symlinks.insert(".claude/agents".into(), ".agents/agents".into());
        symlinks.insert(".claude/commands".into(), ".agents/commands".into());
        symlinks.insert(".claude/rules".into(), ".agents/rules".into());
        symlinks.insert(".opencode/commands".into(), ".agents/commands".into());
        symlinks.insert(".agent/rules".into(), ".agents/rules".into());
        symlinks.insert(".agent/workflows".into(), ".agents/commands".into());
        symlinks.insert(".agent/skills".into(), ".agents/skills".into());
        symlinks.insert(".mcp.json".into(), ".agents/mcp.json".into());
        symlinks.insert(".copilot/mcp-config.json".into(), ".agents/mcp.json".into());
        symlinks.insert(".pi/mcp.json".into(), ".agents/mcp.json".into());
        symlinks.insert("CLAUDE.md".into(), "AGENTS.md".into());

        let mut generate = BTreeMap::new();
        generate.insert(
            "copilot-agents".into(),
            GenerateSpec {
                from: ".agents/agents".into(),
                to: ".github/agents".into(),
                format: "copilot-agent".into(),
                suffix: Some(".agent.md".into()),
                recursive: false,
                tool: None,
                tools: None,
            },
        );
        generate.insert(
            "copilot-prompts".into(),
            GenerateSpec {
                from: ".agents/commands".into(),
                to: ".github/prompts".into(),
                format: "copilot-prompt".into(),
                suffix: Some(".prompt.md".into()),
                recursive: false,
                tool: None,
                tools: None,
            },
        );
        generate.insert(
            "copilot-instructions".into(),
            GenerateSpec {
                from: ".agents/rules".into(),
                to: ".github/instructions".into(),
                format: "copilot-instructions".into(),
                suffix: Some(".instructions.md".into()),
                recursive: true,
                tool: None,
                tools: None,
            },
        );
        generate.insert(
            "opencode-agents".into(),
            GenerateSpec {
                from: ".agents/agents".into(),
                to: ".opencode/agents".into(),
                format: "opencode-agent".into(),
                suffix: Some(".md".into()),
                recursive: false,
                tool: None,
                tools: None,
            },
        );
        generate.insert(
            "codex-agents".into(),
            GenerateSpec {
                from: ".agents/agents".into(),
                to: ".codex/agents".into(),
                format: "codex-agent".into(),
                suffix: Some(".toml".into()),
                recursive: false,
                tool: None,
                tools: None,
            },
        );

        let mut merge = BTreeMap::new();
        merge.insert(
            "opencode-config".into(),
            MergeSpec {
                to: "opencode.json".into(),
                tool: None,
                tools: None,
            },
        );
        merge.insert(
            "codex-config".into(),
            MergeSpec {
                to: ".codex/config.toml".into(),
                tool: None,
                tools: None,
            },
        );

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
    let content = fs::read_to_string(&path)
        .with_context(|| format!("error: failed to read config: {}", path.display()))?;
    let cfg: Config = serde_yaml::from_str(&content)
        .with_context(|| format!("error: invalid config: {}", path.display()))?;
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
    let text = serde_yaml::to_string(&Config::default())?;
    fs::write(path, text)?;
    Ok(true)
}

pub fn find_root(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(root) = explicit {
        return Ok(root.canonicalize().unwrap_or_else(|_| root.to_path_buf()));
    }
    if let Ok(root) = env::var("AGENT_SWITCH_ROOT").or_else(|_| env::var("AGENTSTITCH_ROOT")) {
        let path = PathBuf::from(root);
        return Ok(path.canonicalize().unwrap_or(path));
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

pub fn parse_tools(
    cli_tool: Option<&str>,
    cli_target: Option<&str>,
) -> Result<Option<Vec<String>>> {
    let value = cli_tool
        .or(cli_target)
        .map(str::to_owned)
        .or_else(|| env::var("AGENT_SWITCH_TOOLS").ok())
        .or_else(|| env::var("AGENTSTITCH_TOOLS").ok());
    let Some(value) = value else {
        return Ok(None);
    };
    if value.trim().is_empty() {
        return Err(anyhow!(
            "error: --tool requires a comma-separated tool list"
        ));
    }
    let tools = value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(validate_tool)
        .collect::<Result<Vec<_>>>()?;
    if tools.is_empty() {
        return Err(anyhow!(
            "error: --tool requires a comma-separated tool list"
        ));
    }
    Ok(Some(tools))
}

pub fn validate_tool(tool: &str) -> Result<String> {
    if SUPPORTED_TOOLS.contains(&tool) {
        Ok(tool.to_string())
    } else {
        Err(anyhow!(
            "error: unknown tool: {}; supported tools: {}",
            tool,
            SUPPORTED_TOOLS.join(", ")
        ))
    }
}

pub fn validate_config(cfg: &Config) -> Result<()> {
    if cfg.version != 1 {
        return Err(anyhow!(
            "error: unsupported config version: {}",
            cfg.version
        ));
    }
    for (id, spec) in &cfg.generate {
        validate_declared_tools(spec.tool.as_ref(), spec.tools.as_ref())
            .with_context(|| format!("error: invalid tools for generate mapping: {id}"))?;
    }
    for (id, spec) in &cfg.merge {
        validate_declared_tools(spec.tool.as_ref(), spec.tools.as_ref())
            .with_context(|| format!("error: invalid tools for merge mapping: {id}"))?;
    }
    Ok(())
}

fn validate_declared_tools(tool: Option<&String>, tools: Option<&Vec<String>>) -> Result<()> {
    if let Some(tool) = tool {
        validate_tool(tool)?;
    }
    if let Some(tools) = tools {
        for tool in tools {
            validate_tool(tool)?;
        }
    }
    Ok(())
}

pub fn generate_selected(id: &str, spec: &GenerateSpec, filter: Option<&[String]>) -> bool {
    selected(infer_generate_tools(id, spec), filter)
}

pub fn merge_selected(id: &str, spec: &MergeSpec, filter: Option<&[String]>) -> bool {
    let explicit = explicit_tools(spec.tool.as_ref(), spec.tools.as_ref());
    let inferred = explicit.unwrap_or_else(|| {
        if id.starts_with("codex-") || spec.to.starts_with(".codex") {
            vec!["codex".into()]
        } else if id.starts_with("opencode-") || spec.to == PathBuf::from("opencode.json") {
            vec!["opencode".into()]
        } else {
            vec![]
        }
    });
    selected(inferred, filter)
}

pub fn symlink_selected(link: &str, target: &str, filter: Option<&[String]>) -> bool {
    selected(infer_symlink_tools(link, target), filter)
}

fn selected(mapping_tools: Vec<String>, filter: Option<&[String]>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    mapping_tools
        .iter()
        .any(|tool| filter.iter().any(|f| f == tool))
}

fn infer_generate_tools(id: &str, spec: &GenerateSpec) -> Vec<String> {
    if let Some(tools) = explicit_tools(spec.tool.as_ref(), spec.tools.as_ref()) {
        return tools;
    }
    for tool in ["codex", "copilot", "opencode"] {
        if id.starts_with(&format!("{tool}-")) {
            return vec![tool.into()];
        }
    }
    match spec.format.as_str() {
        "codex-agent" => vec!["codex".into()],
        "copilot-agent" | "copilot-prompt" | "copilot-instructions" => vec!["copilot".into()],
        "opencode-agent" => vec!["opencode".into()],
        _ => vec![],
    }
}

fn explicit_tools(tool: Option<&String>, tools: Option<&Vec<String>>) -> Option<Vec<String>> {
    if let Some(tools) = tools {
        Some(tools.clone())
    } else {
        tool.map(|tool| vec![tool.clone()])
    }
}

fn infer_symlink_tools(link: &str, target: &str) -> Vec<String> {
    if link.starts_with(".claude/skills") {
        return vec!["claude".into(), "pi".into()];
    }
    if link.starts_with(".claude/") || link == ".mcp.json" || link == "CLAUDE.md" {
        return vec!["claude".into()];
    }
    if link.starts_with(".copilot/") {
        return vec!["copilot".into()];
    }
    if link.starts_with(".opencode/") {
        return vec!["opencode".into()];
    }
    if link.starts_with(".pi/") {
        return vec!["pi".into()];
    }
    if link.starts_with(".agent/") {
        return vec!["antigravity".into()];
    }
    if target.contains(".agents/skills") {
        return vec!["claude".into(), "pi".into()];
    }
    vec![]
}
