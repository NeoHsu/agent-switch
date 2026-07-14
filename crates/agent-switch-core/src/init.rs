//! Repository initialization command implementation.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;

use crate::{
    CommandOutput, Error,
    config::{self, CONFIG_FILE, Config, GeneratedTracking, write_config},
    fs::{io_error, write_if_changed},
    mcp,
    tool::Tool,
};

pub fn run(root: &Path, tools: Option<&str>, force: bool) -> Result<CommandOutput> {
    let selected_tools = tools.map(config::parse_tools).transpose()?;
    let config_path = root.join(CONFIG_FILE);
    let cfg = if config_path.exists() && !force {
        config::load_config(root, None)?.0
    } else {
        filtered_default_config(selected_tools.as_deref())
    };
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
        &format!("{}/mcp.json", crate::fs::repo_path(&cfg.agents_dir)),
        &mut out,
    )?;
    write_sample(
        &agents_dir.join("rules/code-style.md"),
        "---\npaths:\n- \"**/*.rs\"\n---\nUse clear, direct Rust code.\n",
        force,
        &format!(
            "{}/rules/code-style.md",
            crate::fs::repo_path(&cfg.agents_dir)
        ),
        &mut out,
    )?;
    write_sample(
        &agents_dir.join("skills/example-skill/SKILL.md"),
        "---\nname: example-skill\ndescription: Example placeholder skill.\n---\n# Example Skill\n\nUse this as a placeholder skill.\n",
        force,
        &format!(
            "{}/skills/example-skill/SKILL.md",
            crate::fs::repo_path(&cfg.agents_dir)
        ),
        &mut out,
    )?;

    if write_config(&config_path, &cfg, force)? {
        out.push(format!("created  {CONFIG_FILE}"));
    } else {
        out.push(format!("skipped  {CONFIG_FILE}: already exists"));
    }
    update_gitignore_for_config(root, &cfg, &mut out)?;
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

pub fn update_gitignore(root: &Path, out: &mut CommandOutput) -> Result<()> {
    update_gitignore_for_config(root, &Config::default(), out)
}

const GITIGNORE_START: &str = "# >>> agent-switch >>>";
const GITIGNORE_END: &str = "# <<< agent-switch <<<";

pub fn update_gitignore_for_config(
    root: &Path,
    cfg: &Config,
    out: &mut CommandOutput,
) -> Result<()> {
    let path = root.join(".gitignore");
    let current = match fs::read_to_string(&path) {
        Ok(current) => current,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(io_error("read existing file", &path, err)),
    };
    let block = gitignore_block(cfg);
    let next = if let Some(start) = current.find(GITIGNORE_START) {
        let search_from = start + GITIGNORE_START.len();
        let end_rel = current[search_from..].find(GITIGNORE_END).ok_or_else(|| {
            Error::Config(format!(
                "{} contains an agent-switch block without a closing marker",
                path.display()
            ))
        })?;
        let marker_end = search_from + end_rel + GITIGNORE_END.len();
        let suffix_start = if current[marker_end..].starts_with("\r\n") {
            marker_end + 2
        } else if current[marker_end..].starts_with('\n') {
            marker_end + 1
        } else {
            marker_end
        };
        let suffix = &current[suffix_start..];
        let mut next = format!("{}{}\n", &current[..start], block);
        if !suffix.is_empty() {
            if !suffix.starts_with('\n') && !suffix.starts_with("\r\n") {
                next.push('\n');
            }
            next.push_str(suffix);
        }
        next
    } else {
        let mut next = current.trim_end().to_string();
        if !next.is_empty() {
            next.push_str("\n\n");
        }
        next.push_str(&block);
        next.push('\n');
        next
    };

    if write_if_changed(&path, &next)? {
        out.push("updated  .gitignore");
    } else {
        out.push("ok       .gitignore");
    }
    Ok(())
}

fn gitignore_block(cfg: &Config) -> String {
    let mut lines = vec![
        GITIGNORE_START.to_string(),
        "# Agent Switch runtime state".to_string(),
        crate::fs::repo_path(&cfg.manifest),
    ];

    let native_paths = cfg
        .symlinks
        .keys()
        .filter(|link| !Path::new(link).starts_with(&cfg.agents_dir))
        .cloned()
        .collect::<BTreeSet<_>>();
    if !native_paths.is_empty() {
        lines.push(String::new());
        lines.push("# Managed native links/copies".to_string());
        lines.extend(native_paths);
    }

    let mut ignored_outputs = BTreeSet::new();
    for (id, spec) in &cfg.generate {
        if cfg.generated_tracking.get(id) != Some(&GeneratedTracking::Tracked) {
            ignored_outputs.insert(format!("{}/", crate::fs::repo_path(&spec.to)));
        }
    }
    for (id, spec) in &cfg.merge {
        if cfg.generated_tracking.get(id) != Some(&GeneratedTracking::Tracked) {
            ignored_outputs.insert(crate::fs::repo_path(&spec.to));
        }
    }
    if !ignored_outputs.is_empty() {
        lines.push(String::new());
        lines.push(
            "# Ignored generated/merged outputs; set generated_tracking to tracked to commit"
                .to_string(),
        );
        lines.extend(ignored_outputs);
    }

    lines.push(GITIGNORE_END.to_string());
    lines.join("\n")
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
