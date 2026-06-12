use std::{fs, path::Path};

use anyhow::Result;

use crate::{config::write_default_config, fs::write_if_changed, mcp, CommandOutput};

const GITIGNORE_BLOCK: &str = r#"# >>> agentstitch >>>
# AgentStitch runtime state
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

# AgentStitch-managed merge target; remove this line if your team wants to commit OpenCode config
opencode.json
# <<< agentstitch <<<
"#;

pub fn run(root: &Path, tools: Option<&str>, force: bool) -> Result<CommandOutput> {
    let mut out = CommandOutput::default();
    create_dir(root, root.join(".agents/agents").as_path(), force, &mut out)?;
    create_dir(
        root,
        root.join(".agents/commands").as_path(),
        force,
        &mut out,
    )?;
    create_dir(root, root.join(".agents/rules").as_path(), force, &mut out)?;
    create_dir(
        root,
        root.join(".agents/skills/example-skill").as_path(),
        force,
        &mut out,
    )?;

    write_sample(
        root.join("AGENTS.md").as_path(),
        "# Agents\n",
        force,
        "AGENTS.md",
        &mut out,
    )?;
    write_sample(
        root.join(".agents/mcp.json").as_path(),
        &mcp::empty_mcp(),
        force,
        ".agents/mcp.json",
        &mut out,
    )?;
    write_sample(
        root.join(".agents/rules/code-style.md").as_path(),
        "---\npaths:\n- \"**/*.rs\"\n---\nUse clear, direct Rust code.\n",
        force,
        ".agents/rules/code-style.md",
        &mut out,
    )?;
    write_sample(
        root.join(".agents/skills/example-skill/SKILL.md").as_path(),
        "# Example Skill\n\nUse this as a placeholder skill.\n",
        force,
        ".agents/skills/example-skill/SKILL.md",
        &mut out,
    )?;

    if write_default_config(&root.join(".agentstitch.yaml"), force)? {
        out.push("created  .agentstitch.yaml");
    } else {
        out.push("skipped  .agentstitch.yaml: already exists");
    }
    update_gitignore(root, &mut out)?;
    if let Some(tools) = tools {
        out.push(format!("ok       initialized tools: {tools}"));
    }
    Ok(out)
}

fn create_dir(root: &Path, path: &Path, _force: bool, out: &mut CommandOutput) -> Result<()> {
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
    if current.contains("# >>> agentstitch >>>") {
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
