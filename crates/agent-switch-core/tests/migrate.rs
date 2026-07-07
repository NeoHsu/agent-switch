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
    let command = fs::read_to_string(root.join(".agents/commands/fix.md")).unwrap();
    assert!(command.contains("name: fix"));
    assert!(command.contains("Fix the bug."));
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
fn migrate_preserves_dotted_copilot_names_and_native_suffixes() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join(".github/agents/speckit.git.commit.agent.md"),
        "---\ndescription: Commits spec work.\n---\nCommit the work.\n",
    );
    write(
        &root.join(".github/prompts/speckit.plan.prompt.md"),
        "---\ndescription: Plans spec work.\n---\nPlan the work.\n",
    );
    write(
        &root.join(".claude/agents/tps2.orchestrator.agent.md"),
        "---\ndescription: Orchestrates TPS2.\n---\nOrchestrate.\n",
    );

    migrate::run(
        root,
        None,
        Some(&[Tool::Claude, Tool::Copilot]),
        migrate::MigrateOptions {
            no_setup: true,
            ..Default::default()
        },
    )
    .unwrap();

    let agent = fs::read_to_string(root.join(".agents/agents/speckit.git.commit.md")).unwrap();
    assert!(agent.contains("name: speckit.git.commit"));
    let command = fs::read_to_string(root.join(".agents/commands/speckit.plan.md")).unwrap();
    assert!(command.contains("name: speckit.plan"));
    let claude_agent =
        fs::read_to_string(root.join(".agents/agents/tps2.orchestrator.md")).unwrap();
    assert!(claude_agent.contains("name: tps2.orchestrator"));
    assert!(
        !root
            .join(".agents/agents/tps2.orchestrator.agent.md")
            .exists()
    );
}

#[test]
fn migrate_prefers_copilot_instruction_over_claude_pointer_rule() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join(".github/instructions/go.instructions.md"),
        "---\napplyTo: \"**/*.go\"\n---\nUse the full Go coding standard.\n",
    );
    write(
        &root.join(".claude/rules/go.md"),
        "---\npaths:\n- \"**/*.go\"\n---\nFollow .github/instructions/go.instructions.md\n",
    );

    migrate::run(
        root,
        None,
        Some(&[Tool::Claude, Tool::Copilot]),
        migrate::MigrateOptions {
            no_setup: true,
            ..Default::default()
        },
    )
    .unwrap();

    let rule = fs::read_to_string(root.join(".agents/rules/go.md")).unwrap();
    assert!(rule.contains("Use the full Go coding standard."));
    assert!(rule.contains("paths:"));
    assert!(!rule.contains("Follow .github/instructions"));
}

#[test]
fn migrate_imports_opencode_and_pi_sources() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(&root.join(".opencode/commands/build.md"), "Build it.\n");
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
        Some(&[Tool::Opencode, Tool::Pi]),
        migrate::MigrateOptions {
            no_setup: true,
            ..Default::default()
        },
    )
    .unwrap();

    let command = fs::read_to_string(root.join(".agents/commands/build.md")).unwrap();
    assert!(command.contains("name: build"));
    assert!(command.contains("Build it."));
    assert!(root.join(".opencode/commands.bak/build.md").exists());
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
