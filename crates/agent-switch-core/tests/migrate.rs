use std::{fs, path::Path};

use agent_switch_core::{
    ExitCode,
    config::{self, SymlinkSpec},
    migrate,
    setup::{self, SetupOptions},
    tool::Tool,
};
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
fn migrate_copilot_imports_workspace_mcp_file() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join(".mcp.json"),
        r#"{
  "mcpServers": {
    "demo": {
      "command": "node",
      "args": ["server.js"]
    }
  }
}
"#,
    );

    migrate::run(
        root,
        None,
        Some(&[Tool::Copilot]),
        migrate::MigrateOptions::default(),
    )
    .unwrap();

    assert!(root.join(".mcp.json.bak").exists());
    assert_managed_source(&root.join(".mcp.json"));
    assert!(!root.join(".copilot/mcp-config.json").exists());
    let mcp: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".agents/mcp.json")).unwrap()).unwrap();
    assert_eq!(mcp["mcpServers"]["demo"]["command"], "node");
    assert_eq!(mcp["mcpServers"]["demo"]["args"][0], "server.js");
}

#[test]
fn migrate_imports_and_retires_legacy_copilot_mcp_config() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join(".copilot/mcp-config.json"),
        r#"{
  "mcpServers": {
    "demo": {
      "type": "local",
      "command": "node",
      "args": ["server.js"],
      "tools": ["*"]
    }
  }
}
"#,
    );

    migrate::run(
        root,
        None,
        Some(&[Tool::Copilot]),
        migrate::MigrateOptions::default(),
    )
    .unwrap();

    assert!(root.join(".copilot/mcp-config.json.bak").exists());
    assert!(!root.join(".copilot/mcp-config.json").exists());
    assert_managed_source(&root.join(".mcp.json"));
    let mcp: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".agents/mcp.json")).unwrap()).unwrap();
    assert_eq!(mcp["mcpServers"]["demo"]["command"], "node");
    assert_eq!(mcp["mcpServers"]["demo"]["args"][0], "server.js");
    assert!(mcp["mcpServers"]["demo"].get("type").is_none());
    assert!(mcp["mcpServers"]["demo"].get("tools").is_none());
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
        &root.join(".pi/prompts/release.md"),
        "Prepare a release for $1.\n",
    );
    write(
        &root.join(".pi/skills/check/SKILL.md"),
        "---\nname: check\ndescription: Check a change\n---\nCheck it.\n",
    );
    write(
        &root.join(".pi/extensions/status.ts"),
        "export default function status() {}\n",
    );
    write(
        &root.join(".pi/themes/contrast.json"),
        "{\"name\":\"contrast\"}\n",
    );
    write(
        &root.join(".pi/settings.json"),
        "{\"packages\":[\"npm:demo\"]}\n",
    );

    migrate::run(
        root,
        None,
        Some(&[Tool::Opencode, Tool::Pi]),
        migrate::MigrateOptions::default(),
    )
    .unwrap();

    let command = fs::read_to_string(root.join(".agents/commands/build.md")).unwrap();
    assert!(command.contains("name: build"));
    assert!(command.contains("Build it."));
    let pi_prompt = fs::read_to_string(root.join(".agents/commands/release.md")).unwrap();
    assert!(pi_prompt.contains("name: release"));
    assert!(pi_prompt.contains("Prepare a release for $1."));
    assert!(root.join(".agents/skills/check/SKILL.md").exists());
    assert!(!root.join(".agents/pi").exists());
    assert!(root.join(".opencode/commands.bak/build.md").exists());
    assert!(root.join(".pi/prompts.bak/release.md").exists());
    assert!(root.join(".pi/skills.bak/check/SKILL.md").exists());
    assert!(!root.join(".pi/extensions.bak").exists());
    assert!(!root.join(".pi/themes.bak").exists());
    assert!(!root.join(".pi/settings.json.bak").exists());
    assert_eq!(
        fs::read_to_string(root.join(".pi/extensions/status.ts")).unwrap(),
        "export default function status() {}\n"
    );
    assert_eq!(
        fs::read_to_string(root.join(".pi/themes/contrast.json")).unwrap(),
        "{\"name\":\"contrast\"}\n"
    );
    assert_eq!(
        fs::read_to_string(root.join(".pi/settings.json")).unwrap(),
        "{\"packages\":[\"npm:demo\"]}\n"
    );
    assert_managed_source(&root.join(".pi/prompts"));
    assert!(!root.join(".pi/skills").exists());
    let mcp: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".agents/mcp.json")).unwrap()).unwrap();
    assert_eq!(mcp["mcpServers"]["local"]["command"], "npx");
    assert_eq!(mcp["mcpServers"]["local"]["args"][0], "demo-mcp");
}

#[test]
fn migrate_antigravity_imports_legacy_paths_into_native_canonical_paths() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join(".agent/rules/review.md"),
        "Review changes carefully.\n",
    );
    write(
        &root.join(".agent/skills/check/SKILL.md"),
        "---\nname: check\ndescription: Checks a change.\n---\nCheck it.\n",
    );
    write(
        &root.join(".agent/workflows/release.md"),
        "Prepare the release.\n",
    );
    write(
        &root.join(".agents/mcp_config.json"),
        r#"{
  "mcpServers": {
    "remote": {
      "serverUrl": "https://example.com/mcp",
      "headers": {"Authorization": "Bearer token"}
    }
  }
}
"#,
    );

    migrate::run(
        root,
        None,
        Some(&[Tool::Antigravity]),
        migrate::MigrateOptions::default(),
    )
    .unwrap();

    assert!(root.join(".agents/rules/review.md").exists());
    assert!(root.join(".agents/skills/check/SKILL.md").exists());
    assert!(root.join(".agents/commands/release.md").exists());
    assert!(root.join(".agent/rules.bak/review.md").exists());
    assert!(root.join(".agent/skills.bak/check/SKILL.md").exists());
    assert!(root.join(".agent/workflows.bak/release.md").exists());
    assert!(!root.join(".agent/rules").exists());
    assert!(!root.join(".agent/skills").exists());
    assert_managed_source(&root.join(".agent/workflows"));
    let canonical_mcp: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".agents/mcp.json")).unwrap()).unwrap();
    assert_eq!(
        canonical_mcp["mcpServers"]["remote"]["url"],
        "https://example.com/mcp"
    );
    let native_mcp: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".agents/mcp_config.json")).unwrap())
            .unwrap();
    assert_eq!(
        native_mcp["mcpServers"]["remote"]["serverUrl"],
        "https://example.com/mcp"
    );
}

#[test]
fn migrate_backs_up_managed_legacy_links_removed_from_config() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    config::write_default_config(&root.join(".agent-switch.yaml"), false).unwrap();
    write(&root.join(".agents/rules/review.md"), "Review carefully.\n");
    write(
        &root.join(".agents/skills/check/SKILL.md"),
        "---\nname: check\ndescription: Checks changes.\n---\nCheck it.\n",
    );
    let cfg = config::load_config(root, None).unwrap().0;
    let mut legacy_cfg = cfg.clone();
    for (link, target) in [
        (".agent/rules", ".agents/rules"),
        (".agent/skills", ".agents/skills"),
    ] {
        legacy_cfg
            .symlinks
            .insert(link.into(), SymlinkSpec::Target(target.into()));
    }
    setup::run(
        root,
        &legacy_cfg,
        Some(&[Tool::Antigravity]),
        SetupOptions {
            no_sync: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_managed_source(&root.join(".agent/rules"));
    assert_managed_source(&root.join(".agent/skills"));

    migrate::run(
        root,
        None,
        Some(&[Tool::Antigravity]),
        migrate::MigrateOptions::default(),
    )
    .unwrap();

    assert!(!root.join(".agent/rules").exists());
    assert!(!root.join(".agent/skills").exists());
    assert!(root.join(".agent/rules.bak").exists());
    assert!(root.join(".agent/skills.bak").exists());
}

#[test]
fn migrate_sets_up_empty_pi_shared_resource_paths() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    migrate::run(
        root,
        None,
        Some(&[Tool::Pi]),
        migrate::MigrateOptions::default(),
    )
    .unwrap();

    assert!(!root.join(".agents/pi").exists());
    assert_managed_source(&root.join(".pi/prompts"));
    assert!(!root.join(".pi/skills").exists());
    assert!(!root.join(".pi/extensions").exists());
    assert!(!root.join(".pi/themes").exists());
    assert!(!root.join(".pi/settings.json").exists());
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
