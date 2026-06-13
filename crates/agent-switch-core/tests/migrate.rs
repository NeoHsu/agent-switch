use std::{fs, path::Path};

use agent_switch_core::{ExitCode, config, migrate, tool::Tool};
use tempfile::tempdir;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

#[cfg(unix)]
fn assert_managed_source(path: &Path) {
    assert!(
        path.is_symlink(),
        "expected managed source path: {}",
        path.display()
    );
}

#[cfg(not(unix))]
fn assert_managed_source(path: &Path) {
    assert!(
        path.exists() || path.is_symlink(),
        "expected managed source path or managed copy: {}",
        path.display()
    );
}

#[test]
fn migrate_claude_imports_native_files_and_sets_up_links() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(&root.join("CLAUDE.md"), "# Existing Claude instructions\n");
    write(&root.join(".claude/commands/fix.md"), "Fix the bug.\n");
    write(
        &root.join(".claude/agents/reviewer.md"),
        "---\nname: reviewer\ndescription: Reviews code.\n---\nReview carefully.\n",
    );
    write(
        &root.join(".mcp.json"),
        r#"{
  "mcpServers": {
    "context7": {
      "command": "npx",
      "args": ["-y", "@upstash/context7-mcp"]
    }
  }
}
"#,
    );

    let out = migrate::run(
        root,
        None,
        Some(&[Tool::Claude]),
        migrate::MigrateOptions::default(),
    )
    .unwrap();

    assert!(root.join(".agent-switch.yaml").exists());
    assert_eq!(
        fs::read_to_string(root.join("AGENTS.md")).unwrap(),
        "# Existing Claude instructions\n"
    );
    assert_eq!(
        fs::read_to_string(root.join(".agents/commands/fix.md")).unwrap(),
        "Fix the bug.\n"
    );
    assert!(root.join("CLAUDE.md.bak").exists());
    assert!(root.join(".claude/commands.bak/fix.md").exists());
    assert!(root.join("CLAUDE.md").exists() || root.join("CLAUDE.md").is_symlink());
    assert!(root.join(".claude/commands").exists() || root.join(".claude/commands").is_symlink());
    let mcp: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".agents/mcp.json")).unwrap()).unwrap();
    assert_eq!(mcp["mcpServers"]["context7"]["command"], "npx");
    assert!(
        out.lines
            .iter()
            .any(|line| { line == "imported CLAUDE.md -> AGENTS.md" })
    );
}

#[test]
fn migrate_skips_managed_native_placeholders() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join("AGENTS.md"),
        "# Existing canonical instructions\n",
    );
    write(&root.join("CLAUDE.md"), "AGENTS.md\n");

    let out = migrate::run(
        root,
        None,
        Some(&[Tool::Claude]),
        migrate::MigrateOptions {
            no_setup: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(
        fs::read_to_string(root.join("AGENTS.md")).unwrap(),
        "# Existing canonical instructions\n"
    );
    assert!(!root.join("CLAUDE.md.bak").exists());
    assert!(
        !out.lines
            .iter()
            .any(|line| line.contains("imported CLAUDE.md -> AGENTS.md"))
    );
}

#[test]
fn migrate_imports_generated_tool_formats_into_one_canonical_agent() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join(".github/agents/reviewer.agent.md"),
        "---\nname: reviewer\ndescription: Reviews code.\ntheme: blue\n---\nReview carefully.\n",
    );
    write(
        &root.join(".codex/agents/reviewer.toml"),
        "name = \"reviewer\"\ndescription = \"Reviews code.\"\nsandbox_mode = \"read-only\"\ndeveloper_instructions = \"Review carefully.\"\n",
    );

    migrate::run(
        root,
        None,
        Some(&[Tool::Copilot, Tool::Codex]),
        migrate::MigrateOptions {
            no_setup: true,
            ..Default::default()
        },
    )
    .unwrap();

    let canonical = fs::read_to_string(root.join(".agents/agents/reviewer.md")).unwrap();
    assert!(canonical.contains("name: reviewer"));
    assert!(canonical.contains("copilot:"));
    assert!(canonical.contains("theme: blue"));
    assert!(canonical.contains("codex:"));
    assert!(canonical.contains("sandbox_mode: read-only"));
}

#[test]
fn migrate_imports_opencode_pi_and_antigravity_sources() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(&root.join(".opencode/commands/build.md"), "Build it.\n");
    write(&root.join(".agent/rules/security.md"), "Keep it safe.\n");
    write(&root.join(".agent/workflows/ship.md"), "Ship it.\n");
    write(
        &root.join(".agent/skills/demo/SKILL.md"),
        "# Demo\n\nUse demo skill.\n",
    );
    write(
        &root.join("opencode.json"),
        r#"{
  "mcp": {
    "local": {
      "type": "local",
      "command": ["npx", "demo-mcp"],
      "environment": {"TOKEN": "${TOKEN}"}
    }
  }
}
"#,
    );
    write(
        &root.join(".pi/mcp.json"),
        r#"{
  "mcpServers": {
    "pi-server": {"command": "node"}
  }
}
"#,
    );

    migrate::run(
        root,
        None,
        Some(&[Tool::Opencode, Tool::Pi, Tool::Antigravity]),
        migrate::MigrateOptions {
            no_setup: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(
        fs::read_to_string(root.join(".agents/commands/build.md")).unwrap(),
        "Build it.\n"
    );
    assert_eq!(
        fs::read_to_string(root.join(".agents/commands/ship.md")).unwrap(),
        "Ship it.\n"
    );
    assert_eq!(
        fs::read_to_string(root.join(".agents/rules/security.md")).unwrap(),
        "Keep it safe.\n"
    );
    assert!(root.join(".agents/skills/demo/SKILL.md").exists());
    assert!(root.join(".opencode/commands.bak/build.md").exists());
    assert!(root.join(".agent/rules.bak/security.md").exists());
    assert!(root.join(".pi/mcp.json.bak").exists());
    let mcp: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".agents/mcp.json")).unwrap()).unwrap();
    assert_eq!(mcp["mcpServers"]["local"]["command"], "npx");
    assert_eq!(mcp["mcpServers"]["local"]["args"][0], "demo-mcp");
    assert_eq!(mcp["mcpServers"]["pi-server"]["command"], "node");
}

#[test]
fn migrate_is_idempotent_after_setup() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(&root.join("CLAUDE.md"), "# Existing Claude instructions\n");

    migrate::run(
        root,
        None,
        Some(&[Tool::Claude]),
        migrate::MigrateOptions::default(),
    )
    .unwrap();

    // After the first migrate+setup pass, CLAUDE.md is a managed source path.
    assert_managed_source(&root.join("CLAUDE.md"));

    let out = migrate::run(
        root,
        None,
        Some(&[Tool::Claude]),
        migrate::MigrateOptions::default(),
    )
    .unwrap();

    // Re-running must not back up the already-managed source into a second
    // stray .bak (the original real file already produced CLAUDE.md.bak).
    assert!(!root.join("CLAUDE.md.bak.1").exists());
    assert_managed_source(&root.join("CLAUDE.md"));
    assert!(
        !out.lines
            .iter()
            .any(|line| line.starts_with("backed up CLAUDE.md"))
    );
}

#[test]
fn migrate_check_reports_drift_without_writing() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(&root.join("CLAUDE.md"), "# Existing Claude instructions\n");

    let out = migrate::run(
        root,
        None,
        Some(&[Tool::Claude]),
        migrate::MigrateOptions {
            check: true,
            no_setup: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(out.exit(), ExitCode::Drift);
    assert!(!root.join(".agent-switch.yaml").exists());
    assert!(!root.join("AGENTS.md").exists());
    assert!(!root.join("CLAUDE.md.bak").exists());
}

#[test]
fn migrate_check_reports_drift_for_conflict_without_overwrite() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    config::write_config(
        &root.join(".agent-switch.yaml"),
        &config::Config::default(),
        false,
    )
    .unwrap();
    write(&root.join("CLAUDE.md"), "# Existing Claude instructions\n");
    write(&root.join("AGENTS.md"), "# Existing canonical agents\n");

    let out = migrate::run(
        root,
        None,
        Some(&[Tool::Claude]),
        migrate::MigrateOptions {
            check: true,
            no_setup: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(out.exit(), ExitCode::Drift);
    assert!(
        out.lines
            .iter()
            .any(|line| line.contains("skipped  AGENTS.md: already exists"))
    );
}
