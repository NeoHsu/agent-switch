use std::{fs, path::Path};

use agentstitch_core::{
    config::{self, Config},
    sync::{self, SyncOptions},
    ExitCode,
};
use tempfile::tempdir;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn fixture(root: &Path) -> Config {
    config::write_default_config(&root.join(".agentstitch.yaml"), false).unwrap();
    write(
        &root.join(".agents/agents/reviewer.md"),
        r#"---
name: reviewer
description: Reviews code changes.
tools: Read, Grep
model: sonnet
copilot:
  infer: false
opencode:
  model: anthropic/claude-sonnet-4-6
codex:
  sandbox_mode: read-only
---
Review the diff.
"#,
    );
    write(
        &root.join(".agents/commands/fix.md"),
        r#"---
name: fix
description: Fix an issue.
copilot:
  mode: agent
---
Fix the issue.
"#,
    );
    write(
        &root.join(".agents/rules/testing/unit.md"),
        r#"---
description: Unit testing rules.
paths:
- "src/**/*.rs"
- "tests/**/*.rs"
---
Write focused tests.
"#,
    );
    write(
        &root.join(".agents/mcp.json"),
        r#"{
  "mcpServers": {
    "context7": {
      "command": "npx",
      "args": ["-y", "@upstash/context7-mcp"],
      "env": {"KEY": "${KEY}"}
    }
  }
}
"#,
    );
    config::load_config(root, None).unwrap().0
}

#[test]
fn full_sync_generates_outputs_and_check_passes() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);

    let out = sync::run(root, &cfg, None, SyncOptions::default()).unwrap();
    assert!(out
        .lines
        .iter()
        .any(|line| line == "generated: .github/agents/reviewer.agent.md"));
    assert!(root.join(".github/agents/reviewer.agent.md").exists());
    assert!(root.join(".github/prompts/fix.prompt.md").exists());
    assert!(root
        .join(".github/instructions/testing/unit.instructions.md")
        .exists());
    assert!(root.join(".opencode/agents/reviewer.md").exists());
    assert!(root.join(".codex/agents/reviewer.toml").exists());
    assert!(root.join("opencode.json").exists());
    assert!(root.join(".codex/config.toml").exists());
    assert!(root.join(".agents/.sync-manifest.json").exists());

    let second = sync::run(root, &cfg, None, SyncOptions::default()).unwrap();
    assert_eq!(second.lines, vec!["synced, no changes."]);

    let check = sync::run(
        root,
        &cfg,
        None,
        SyncOptions {
            check: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(check.exit(), ExitCode::Ok);
}

#[test]
fn tool_filter_only_generates_selected_tool_outputs() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);

    sync::run(root, &cfg, Some(&["codex".into()]), SyncOptions::default()).unwrap();

    assert!(root.join(".codex/agents/reviewer.toml").exists());
    assert!(root.join(".codex/config.toml").exists());
    assert!(!root.join(".github/agents/reviewer.agent.md").exists());
    assert!(!root.join(".opencode/agents/reviewer.md").exists());
}

#[test]
fn generated_import_conflict_tool_side_wins_and_preserves_other_namespaces() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);

    sync::run(
        root,
        &cfg,
        Some(&["copilot".into()]),
        SyncOptions::default(),
    )
    .unwrap();
    write(
        &root.join(".github/agents/reviewer.agent.md"),
        r#"---
name: reviewer
description: Tool side description.
infer: true
---
Review from generated.
"#,
    );
    write(
        &root.join(".agents/agents/reviewer.md"),
        r#"---
name: reviewer
description: Canonical side description.
tools: Read
model: sonnet
opencode:
  model: kept
codex:
  sandbox_mode: read-only
---
Canonical body changed.
"#,
    );

    let out = sync::run(
        root,
        &cfg,
        Some(&["copilot".into()]),
        SyncOptions::default(),
    )
    .unwrap();
    assert!(out.lines.iter().any(|line| {
        line == "imported(conflict, tool-side wins): .github/agents/reviewer.agent.md -> .agents/agents/reviewer.md"
    }));
    let canonical = fs::read_to_string(root.join(".agents/agents/reviewer.md")).unwrap();
    assert!(canonical.contains("Tool side description."));
    assert!(canonical.contains("Review from generated."));
    assert!(canonical.contains("opencode:"));
    assert!(canonical.contains("codex:"));
    assert!(canonical.contains("tools: Read"));
    assert!(canonical.contains("infer: true"));
}

#[test]
fn stale_generated_file_is_removed_when_source_is_deleted() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);

    sync::run(root, &cfg, Some(&["codex".into()]), SyncOptions::default()).unwrap();
    fs::remove_file(root.join(".agents/agents/reviewer.md")).unwrap();

    let out = sync::run(root, &cfg, Some(&["codex".into()]), SyncOptions::default()).unwrap();
    assert!(out
        .lines
        .iter()
        .any(|line| line == "removed: .codex/agents/reviewer.toml"));
    assert!(!root.join(".codex/agents/reviewer.toml").exists());
}
