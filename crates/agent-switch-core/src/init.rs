use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;

use crate::{
    CommandOutput,
    config::{CONFIG_FILE, Config, write_default_config},
    fs::write_if_changed,
    mcp,
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
    let cfg = Config::default();
    let agents_dir = root.join(&cfg.agents_dir);
    let mut out = CommandOutput::default();

    // Derive directories to create from generate spec source paths (unique,
    // sorted), plus the example-skill directory which has no generate spec.
    let mut dirs: BTreeSet<PathBuf> = cfg.generate.values().map(|s| root.join(&s.from)).collect();
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

    if write_default_config(&root.join(CONFIG_FILE), force)? {
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
        fs::create_dir_all(path)?;
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

fn update_gitignore(root: &Path, out: &mut CommandOutput) -> Result<()> {
    let path = root.join(".gitignore");
    let current = fs::read_to_string(&path).unwrap_or_default();
    if current.contains("# >>> agent-switch >>>") || current.contains("# >>> agentstitch >>>") {
        out.push("ok       .gitignore");
        return Ok(());
    }
    let mut next = current.trim_end().to_string();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str(GITIGNORE_BLOCK);
    next.push('\n');
    fs::write(path, next)?;
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
