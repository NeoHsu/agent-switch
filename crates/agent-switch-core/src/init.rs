//! Repository initialization command implementation.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;

use crate::{
    CommandOutput,
    config::{self, CONFIG_FILE, Config, write_config},
    fs::{io_error, write_if_changed},
    mcp,
    tool::Tool,
};

const GITIGNORE_BLOCK: &str = r#"# >>> agent-switch >>>
# Agent Switch runtime state
.agents/.sync-manifest.json

# Tool-specific links/copies and generated adapters
.claude/
.copilot/
.pi/
.agent/
.codex/
.opencode/
.github/agents/
.github/prompts/
.github/instructions/

# Agent Switch-managed merge target; remove this line if your team wants to commit OpenCode config
opencode.json
# <<< agent-switch <<<
"#;

pub fn run(root: &Path, tools: Option<&str>, force: bool) -> Result<CommandOutput> {
    let selected_tools = tools.map(config::parse_tools).transpose()?;
    let cfg = filtered_default_config(selected_tools.as_deref());
    let agents_dir = root.join(&cfg.agents_dir);
    let mut out = CommandOutput::default();

    // Derive directories to create from selected generate source paths and
    // always create the canonical directories used by built-in symlinks.
    let mut dirs: BTreeSet<PathBuf> = cfg.generate.values().map(|s| root.join(&s.from)).collect();
    dirs.insert(agents_dir.join("agents"));
    dirs.insert(agents_dir.join("commands"));
    dirs.insert(agents_dir.join("rules"));
    dirs.insert(agents_dir.join("skills").join("example-skill"));
    for dir in &dirs {
        create_dir(root, dir, &mut out)?;
    }

    write_sample(
        &root.join("AGENTS.md"),
        "# Agents\n",
        force,
        "AGENTS.md",
        &mut out,
    )?;
    write_sample(
        &agents_dir.join("mcp.json"),
        mcp::EMPTY_MCP,
        force,
        ".agents/mcp.json",
        &mut out,
    )?;
    write_sample(
        &agents_dir.join("rules/code-style.md"),
        "---\npaths:\n- \"**/*.rs\"\n---\nUse clear, direct Rust code.\n",
        force,
        ".agents/rules/code-style.md",
        &mut out,
    )?;
    write_sample(
        &agents_dir.join("skills/example-skill/SKILL.md"),
        "# Example Skill\n\nUse this as a placeholder skill.\n",
        force,
        ".agents/skills/example-skill/SKILL.md",
        &mut out,
    )?;

    if write_config(&root.join(CONFIG_FILE), &cfg, force)? {
        out.push(format!("created  {CONFIG_FILE}"));
    } else {
        out.push(format!("skipped  {CONFIG_FILE}: already exists"));
    }
    update_gitignore(root, &mut out)?;
    if let Some(tools) = tools {
        out.push(format!("ok       initialized tools: {tools}"));
    }
    Ok(out)
}

fn create_dir(root: &Path, path: &Path, out: &mut CommandOutput) -> Result<()> {
    if path.exists() {
        out.push(format!("ok       {}", rel(root, path)));
    } else {
        fs::create_dir_all(path).map_err(|err| io_error("create directory", path, err))?;
        out.push(format!("created  {}", rel(root, path)));
    }
    Ok(())
}

fn write_sample(
    path: &Path,
    content: &str,
    force: bool,
    display: &str,
    out: &mut CommandOutput,
) -> Result<()> {
    if path.exists() && !force {
        out.push(format!("skipped  {display}: already exists"));
        return Ok(());
    }
    write_if_changed(path, content)?;
    out.push(format!("created  {display}"));
    Ok(())
}

fn filtered_default_config(tools: Option<&[Tool]>) -> Config {
    let mut cfg = Config::default();
    if let Some(tools) = tools {
        cfg.symlinks
            .retain(|link, spec| config::symlink_selected(link, spec, Some(tools)));
        cfg.generate
            .retain(|_, spec| config::generate_selected(spec, Some(tools)));
        cfg.merge
            .retain(|id, spec| config::merge_selected(id, spec, Some(tools)));
    }
    cfg
}

fn update_gitignore(root: &Path, out: &mut CommandOutput) -> Result<()> {
    let path = root.join(".gitignore");
    let current = match fs::read_to_string(&path) {
        Ok(current) => current,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(io_error("read existing file", &path, err)),
    };
    if current.contains("# >>> agent-switch >>>") {
        out.push("ok       .gitignore");
        return Ok(());
    }
    let mut next = current.trim_end().to_string();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str(GITIGNORE_BLOCK);
    next.push('\n');
    write_if_changed(&path, &next)?;
    out.push("updated  .gitignore");
    Ok(())
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
