use std::{fs, path::Path};

use serde_json::json;

use agent_switch_core::{
    Error, ExitCode,
    config::{self, Config, SyncMode},
    manifest,
    sync::{self, SyncOptions},
    tool::Tool,
};
use tempfile::tempdir;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn fixture(root: &Path) -> Config {
    config::write_default_config(&root.join(".agent-switch.yaml"), false).unwrap();
    write(
        &root.join(".agent/agents/reviewer.md"),
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
        &root.join(".agent/commands/fix.md"),
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
        &root.join(".agent/rules/testing/unit.md"),
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
        &root.join(".agent/mcp.json"),
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
    let mut cfg = config::load_config(root, None).unwrap().0;
    cfg.sync_mode = SyncMode::Full;
    cfg
}

#[test]
fn full_sync_generates_outputs_and_check_passes() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);

    let out = sync::run(root, &cfg, None, SyncOptions::default()).unwrap();
    assert!(
        out.lines
            .iter()
            .any(|line| line == "generated: .github/agents/reviewer.agent.md")
    );
    assert!(root.join(".github/agents/reviewer.agent.md").exists());
    assert!(root.join(".github/prompts/fix.prompt.md").exists());
    assert!(
        root.join(".github/instructions/testing/unit.instructions.md")
            .exists()
    );
    assert!(root.join(".opencode/agents/reviewer.md").exists());
    assert!(root.join(".codex/agents/reviewer.toml").exists());
    assert!(root.join("opencode.json").exists());
    assert!(root.join(".codex/config.toml").exists());
    assert!(root.join(".copilot/mcp-config.json").exists());
    assert!(root.join(".agent/.sync-manifest.json").exists());

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
fn sync_reports_manifest_recovery_hint() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write(&root.join(".agent/.sync-manifest.json"), "{not json\n");

    let err = sync::run(root, &cfg, None, SyncOptions::default()).unwrap_err();
    let message = format!("{err:#}");

    assert!(message.contains("failed to read manifest .agent/.sync-manifest.json"));
    assert!(message.contains("Run `ags sync --reset-manifest` to rebuild it."));
}

#[test]
fn sync_reset_manifest_rebuilds_corrupt_manifest() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    let manifest_path = root.join(".agent/.sync-manifest.json");
    write(&manifest_path, "{not json\n");

    let out = sync::run(
        root,
        &cfg,
        None,
        SyncOptions {
            reset_manifest: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(out.lines.iter().any(|line| {
        line == "warning: reset manifest: rebuilding .agent/.sync-manifest.json from current files"
    }));
    let rebuilt = manifest::load(&manifest_path).unwrap();
    assert!(!rebuilt.generated.is_empty());
}

#[test]
fn sync_reset_manifest_check_reports_drift_without_writing() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    let manifest_path = root.join(".agent/.sync-manifest.json");
    write(&manifest_path, "{not json\n");

    let out = sync::run(
        root,
        &cfg,
        None,
        SyncOptions {
            check: true,
            reset_manifest: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(out.exit(), ExitCode::Drift);
    assert_eq!(fs::read_to_string(&manifest_path).unwrap(), "{not json\n");
}

#[test]
fn sync_can_output_machine_readable_json() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);

    let out = sync::run(
        root,
        &cfg,
        None,
        SyncOptions {
            json: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(out.lines.len(), 1);
    let report: serde_json::Value = serde_json::from_str(&out.lines[0]).unwrap();
    assert_eq!(report["exit"], json!("Ok"));
    assert_eq!(report["exit_code"].as_i64().unwrap(), 0);
    assert!(report["options"]["json"].as_bool().unwrap());
    assert!(!report["options"]["reset_manifest"].as_bool().unwrap());
    assert!(!report["events"].as_array().unwrap().is_empty());
    assert_eq!(
        report["summary"]["total_events"].as_u64().unwrap() as usize,
        report["events"].as_array().unwrap().len()
    );
    assert!(
        report["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["event"] == json!("generated"))
    );
    assert!(
        report["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["sequence"].as_u64().is_some_and(|sequence| sequence > 0))
    );
    assert!(
        report["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["event"] == json!("merged"))
    );

    let check = sync::run(
        root,
        &cfg,
        None,
        SyncOptions {
            check: true,
            json: true,
            ..Default::default()
        },
    )
    .unwrap();
    let report: serde_json::Value = serde_json::from_str(&check.lines[0]).unwrap();
    assert_eq!(report["exit_code"].as_i64().unwrap(), 0);
    assert!(
        report["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["event"] == json!("synced_no_changes"))
    );
}

#[test]
fn sync_can_filter_json_events_by_kind() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);

    let filter =
        sync::parse_event_filter(&["generated".to_string(), "merged".to_string()]).unwrap();
    let out = sync::run(
        root,
        &cfg,
        None,
        SyncOptions {
            json: true,
            event_filter: Some(filter),
            ..Default::default()
        },
    )
    .unwrap();

    let report: serde_json::Value = serde_json::from_str(&out.lines[0]).unwrap();
    let events = report["events"].as_array().unwrap();
    assert!(!events.is_empty());
    assert_eq!(
        report["options"]["event_filter"],
        json!(["generated", "merged"])
    );
    assert!(events.iter().all(|e| {
        let kind = e["event"].as_str();
        matches!(kind, Some("generated") | Some("merged"))
    }));
}

#[test]
fn sync_json_events_are_sorted_stably() {
    let first = {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let cfg = fixture(root);
        sync::run(
            root,
            &cfg,
            None,
            SyncOptions {
                json: true,
                ..Default::default()
            },
        )
        .unwrap()
    };

    let second = {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let cfg = fixture(root);
        sync::run(
            root,
            &cfg,
            None,
            SyncOptions {
                json: true,
                ..Default::default()
            },
        )
        .unwrap()
    };

    assert_eq!(first.lines, second.lines);

    let report: serde_json::Value = serde_json::from_str(&first.lines[0]).unwrap();
    let order = report["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["event"].as_str().unwrap_or_default().to_string());
    let order: Vec<String> = order.collect();
    let expected = [
        "imported",
        "generated",
        "removed",
        "copied",
        "warning",
        "merged",
        "drift",
        "synced_no_changes",
    ];

    let mut last_index = 0;
    for event in order {
        let idx = expected
            .iter()
            .position(|kind| kind == &event.as_str())
            .unwrap();
        assert!(idx >= last_index, "event order is not stable: {event}");
        last_index = idx;
    }
}

#[test]
fn tool_filter_only_generates_selected_tool_outputs() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);

    sync::run(root, &cfg, Some(&[Tool::Codex]), SyncOptions::default()).unwrap();

    assert!(root.join(".codex/agents/reviewer.toml").exists());
    assert!(root.join(".codex/config.toml").exists());
    assert!(!root.join(".copilot/mcp-config.json").exists());
    assert!(!root.join(".github/agents/reviewer.agent.md").exists());
    assert!(!root.join(".opencode/agents/reviewer.md").exists());
}

#[test]
fn generated_import_conflict_tool_side_wins_and_preserves_other_namespaces() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);

    sync::run(root, &cfg, Some(&[Tool::Copilot]), SyncOptions::default()).unwrap();
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
        &root.join(".agent/agents/reviewer.md"),
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

    let out = sync::run(root, &cfg, Some(&[Tool::Copilot]), SyncOptions::default()).unwrap();
    assert!(out.lines.iter().any(|line| {
        line == "imported(conflict, tool-side wins): .github/agents/reviewer.agent.md -> .agent/agents/reviewer.md"
    }));
    let canonical = fs::read_to_string(root.join(".agent/agents/reviewer.md")).unwrap();
    assert!(canonical.contains("Tool side description."));
    assert!(canonical.contains("Review from generated."));
    assert!(canonical.contains("opencode:"));
    assert!(canonical.contains("codex:"));
    assert!(canonical.contains("tools: Read"));
    assert!(canonical.contains("infer: true"));
}

#[test]
fn sync_rejects_mutually_exclusive_mode_flags() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);

    let err = sync::run(
        root,
        &cfg,
        None,
        SyncOptions {
            import_only: true,
            export_only: true,
            ..Default::default()
        },
    )
    .unwrap_err();

    let config_err = err
        .downcast_ref::<Error>()
        .expect("expected a config error from invalid sync options");
    assert!(matches!(config_err, Error::Config(_)));
}

#[test]
fn sync_generates_outputs_with_uppercase_markdown_sources() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);

    write(
        &root.join(".agent/agents/UPPER.MD"),
        r#"---
name: uppercase
description: Source with uppercase extension.
---
Hello from uppercase extension.
"#,
    );

    let out = sync::run(root, &cfg, None, SyncOptions::default()).unwrap();

    assert!(
        out.lines
            .iter()
            .any(|line| line == "generated: .github/agents/UPPER.agent.md")
    );
    assert!(root.join(".github/agents/UPPER.agent.md").exists());
}

#[test]
fn stale_generated_file_is_removed_when_source_is_deleted() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);

    sync::run(root, &cfg, Some(&[Tool::Codex]), SyncOptions::default()).unwrap();
    fs::remove_file(root.join(".agent/agents/reviewer.md")).unwrap();

    let out = sync::run(root, &cfg, Some(&[Tool::Codex]), SyncOptions::default()).unwrap();
    assert!(
        out.lines
            .iter()
            .any(|line| line == "removed: .codex/agents/reviewer.toml")
    );
    assert!(!root.join(".codex/agents/reviewer.toml").exists());
}

#[test]
fn sync_does_not_overwrite_unmanaged_link_files() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write(&root.join("AGENTS.md"), "# Canonical\n");
    write(&root.join("CLAUDE.md"), "# Private notes\n");

    let out = sync::run(root, &cfg, None, SyncOptions::default()).unwrap();

    assert_eq!(
        fs::read_to_string(root.join("AGENTS.md")).unwrap(),
        "# Canonical\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("CLAUDE.md")).unwrap(),
        "# Private notes\n"
    );
    assert!(
        out.lines
            .iter()
            .any(|line| { line.starts_with("warning: CLAUDE.md is an unmanaged real file") })
    );
    let tracked = manifest::load(&root.join(".agents/.sync-manifest.json")).unwrap();
    assert!(!tracked.links.contains_key("CLAUDE.md"));

    // Once generated outputs are in sync, an unmanaged file that sync cannot
    // fix must not flag drift in check mode.
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
fn sync_recreates_missing_manifest_tracked_copy() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    let canonical = fs::read_to_string(root.join(".agents/mcp.json")).unwrap();
    let mut tracked = manifest::Manifest::default();
    tracked
        .links
        .insert(".pi/mcp.json".into(), manifest::sha256_text(&canonical));
    manifest::save(&root.join(".agents/.sync-manifest.json"), &mut tracked).unwrap();

    let out = sync::run(root, &cfg, None, SyncOptions::default()).unwrap();

    assert!(
        out.lines
            .iter()
            .any(|line| line == "copied: .agents/mcp.json -> .pi/mcp.json")
    );
    assert_eq!(
        fs::read_to_string(root.join(".pi/mcp.json")).unwrap(),
        canonical
    );
}

