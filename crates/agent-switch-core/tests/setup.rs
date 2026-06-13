use std::{fs, path::Path};

use agent_switch_core::{
    ExitCode,
    config::{self, Config, SymlinkDetail, SymlinkSpec},
    diagnostics, init,
    manifest::{self, Manifest},
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
fn assert_managed_link(path: &Path) {
    assert!(path.is_symlink(), "expected symlink: {}", path.display());
}

#[cfg(not(unix))]
fn assert_managed_link(path: &Path) {
    assert!(
        path.exists() || path.is_symlink(),
        "expected managed link or copy: {}",
        path.display()
    );
}

fn assert_absent(path: &Path) {
    assert!(
        !path.exists() && !path.is_symlink(),
        "expected path to be absent: {}",
        path.display()
    );
}

fn fixture(root: &Path) -> Config {
    config::write_default_config(&root.join(".agent-switch.yaml"), false).unwrap();
    fs::create_dir_all(root.join(".agents/skills")).unwrap();
    fs::create_dir_all(root.join(".agents/agents")).unwrap();
    fs::create_dir_all(root.join(".agents/commands")).unwrap();
    fs::create_dir_all(root.join(".agents/rules")).unwrap();
    write(&root.join(".agents/mcp.json"), "{}\n");
    write(&root.join("AGENTS.md"), "# Agents\n");
    config::load_config(root, None).unwrap().0
}

#[test]
fn init_writes_agent_switch_config() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    let out = init::run(root, None, false).unwrap();

    assert!(root.join(".agent-switch.yaml").exists());
    assert!(
        out.lines
            .iter()
            .any(|line| line == "created  .agent-switch.yaml")
    );
}

#[test]
fn init_with_tools_filters_default_mappings() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    let out = init::run(root, Some("codex"), false).unwrap();
    let cfg = config::load_config(root, None).unwrap().0;

    assert!(
        out.lines
            .iter()
            .any(|line| line == "ok       initialized tools: codex")
    );
    assert!(cfg.symlinks.is_empty());
    assert_eq!(
        cfg.generate.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["codex-agents"]
    );
    assert_eq!(
        cfg.merge.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["codex-config"]
    );
}

#[test]
fn config_loads_detailed_symlink_specs() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join(".agent-switch.yaml"),
        r#"version: 1
symlinks:
  CUSTOM.md:
    to: .agents/custom.md
    tools: [codex]
"#,
    );

    let cfg = config::load_config(root, None).unwrap().0;
    let spec = cfg.symlinks.get("CUSTOM.md").unwrap();

    assert_eq!(spec.target(), Path::new(".agents/custom.md"));
    assert!(config::symlink_selected(
        "CUSTOM.md",
        spec,
        Some(&[Tool::Codex])
    ));
    assert!(!config::symlink_selected(
        "CUSTOM.md",
        spec,
        Some(&[Tool::Claude])
    ));
}

#[test]
fn config_missing_suggests_init() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    let err = config::load_config(root, None).unwrap_err();

    assert_eq!(
        err.to_string(),
        "No config file found. Run 'ags init' to create one."
    );
}

#[test]
fn config_rejects_path_traversal() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join(".agent-switch.yaml"),
        r#"version: 1
agents_dir: ../outside
"#,
    );

    let err = config::load_config(root, None).unwrap_err();

    assert!(err.to_string().contains("path cannot contain `..`"));
}

#[test]
fn config_rejects_ambiguous_tool_selection() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join(".agent-switch.yaml"),
        r#"version: 1
symlinks:
  CUSTOM.md:
    to: .agents/custom.md
    tool: codex
    tools: [copilot]
"#,
    );

    let err = config::load_config(root, None).unwrap_err();

    assert!(err.to_string().contains("use either `tool` or `tools`"));
}

#[test]
fn config_rejects_duplicate_generate_outputs() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join(".agent-switch.yaml"),
        r#"version: 1
generate:
  agents:
    from: .agents/agents
    to: .generated
    format: copilot-agent
  prompts:
    from: .agents/prompts
    to: .generated
    format: copilot-prompt
"#,
    );

    let err = config::load_config(root, None).unwrap_err();

    assert!(err.to_string().contains("duplicate output directory"));
}

#[test]
fn doctor_reports_custom_agents_dir() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join(".agent-switch.yaml"),
        r#"version: 1
agents_dir: custom-agents
"#,
    );
    fs::create_dir_all(root.join("custom-agents")).unwrap();
    let cfg = config::load_config(root, None).unwrap().0;

    let out = diagnostics::doctor(root, Some(&cfg), false).unwrap();

    assert!(
        out.lines
            .iter()
            .any(|line| line == "ok       custom-agents exists")
    );
}

#[test]
fn doctor_reports_manifest_recovery_hint() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write(&root.join(".agents/.sync-manifest.json"), "{not json\n");

    let out = diagnostics::doctor(root, Some(&cfg), false).unwrap();

    assert!(
        out.lines.iter().any(|line| {
            line == "warning: manifest is not parseable: .agents/.sync-manifest.json"
        })
    );
    assert!(out.lines.iter().any(|line| {
        line == "hint:    delete .agents/.sync-manifest.json and run `ags sync` to rebuild it"
    }));
}

#[test]
fn doctor_json_reports_manifest_recovery_hint() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write(&root.join(".agents/.sync-manifest.json"), "{not json\n");

    let out = diagnostics::doctor(root, Some(&cfg), true).unwrap();
    let report: serde_json::Value = serde_json::from_str(&out.lines[0]).unwrap();

    assert_eq!(report["manifest"].as_bool(), Some(false));
    assert_eq!(
        report["manifest_path"].as_str(),
        Some(".agents/.sync-manifest.json")
    );
    assert!(
        report["manifest_error"]
            .as_str()
            .is_some_and(|err| err.contains("manifest is not parseable"))
    );
    assert_eq!(
        report["manifest_recovery"].as_str(),
        Some("Delete .agents/.sync-manifest.json and run `ags sync` to rebuild it.")
    );
}

#[test]
fn setup_check_reports_existing_real_file_as_drift() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write(&root.join(".copilot/mcp-config.json"), "real file\n");

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Copilot]),
        SetupOptions {
            no_sync: true,
            check: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(out.exit(), ExitCode::Drift);
    assert!(out.lines.iter().any(|line| {
        line.starts_with("skipped  .copilot/mcp-config.json: existing real file or directory")
    }));
}

#[test]
fn setup_prune_removes_links_for_unselected_tools() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);

    setup::run(
        root,
        &cfg,
        None,
        SetupOptions {
            no_sync: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_managed_link(&root.join(".copilot/mcp-config.json"));
    assert_managed_link(&root.join(".pi/mcp.json"));
    assert_managed_link(&root.join(".claude/skills"));

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Claude]),
        SetupOptions {
            no_sync: true,
            prune: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(
        out.lines
            .iter()
            .any(|line| line == "removed: .copilot/mcp-config.json")
    );
    assert!(out.lines.iter().any(|line| line == "removed: .pi/mcp.json"));
    assert_absent(&root.join(".copilot/mcp-config.json"));
    assert_absent(&root.join(".pi/mcp.json"));
    assert_managed_link(&root.join(".claude/skills"));
    assert_managed_link(&root.join(".mcp.json"));
}

#[test]
fn setup_prune_keeps_custom_links_without_tool_ownership() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let mut cfg = fixture(root);
    write(&root.join(".agents/custom.md"), "custom\n");
    cfg.symlinks.insert(
        "CUSTOM.md".into(),
        SymlinkSpec::Target(".agents/custom.md".into()),
    );

    setup::run(
        root,
        &cfg,
        None,
        SetupOptions {
            no_sync: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_managed_link(&root.join("CUSTOM.md"));

    setup::run(
        root,
        &cfg,
        Some(&[Tool::Claude]),
        SetupOptions {
            no_sync: true,
            prune: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_managed_link(&root.join("CUSTOM.md"));
}

#[test]
fn setup_prune_honors_custom_link_tool_ownership() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let mut cfg = fixture(root);
    write(&root.join(".agents/custom.md"), "custom\n");
    cfg.symlinks.insert(
        "CUSTOM.md".into(),
        SymlinkSpec::Detailed(SymlinkDetail {
            to: ".agents/custom.md".into(),
            tool: None,
            tools: Some(vec![Tool::Codex]),
        }),
    );

    setup::run(
        root,
        &cfg,
        None,
        SetupOptions {
            no_sync: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_managed_link(&root.join("CUSTOM.md"));

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Claude]),
        SetupOptions {
            no_sync: true,
            prune: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(out.lines.iter().any(|line| line == "removed: CUSTOM.md"));
    assert_absent(&root.join("CUSTOM.md"));
}

#[test]
fn setup_prune_skips_unmanaged_real_directories() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    fs::create_dir_all(root.join(".copilot/mcp-config.json")).unwrap();

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Claude]),
        SetupOptions {
            no_sync: true,
            prune: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(out.lines.iter().any(|line| {
        line.starts_with("skipped  .copilot/mcp-config.json: existing real file or directory")
    }));
    assert!(root.join(".copilot/mcp-config.json").is_dir());
}

#[test]
fn setup_prune_removes_manifest_tracked_copy_fallback() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write(&root.join(".copilot/mcp-config.json"), "{}\n");
    let mut sync_manifest = Manifest::default();
    sync_manifest.links.insert(
        ".copilot/mcp-config.json".into(),
        manifest::sha256_text("{}\n"),
    );
    manifest::save(
        &root.join(".agents/.sync-manifest.json"),
        &mut sync_manifest,
    )
    .unwrap();

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Claude]),
        SetupOptions {
            no_sync: true,
            prune: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(
        out.lines
            .iter()
            .any(|line| line == "removed: .copilot/mcp-config.json")
    );
    assert_absent(&root.join(".copilot/mcp-config.json"));
    let next_manifest = manifest::load(&root.join(".agents/.sync-manifest.json")).unwrap();
    assert!(!next_manifest.links.contains_key(".copilot/mcp-config.json"));
}

#[test]
fn setup_check_prune_reports_drift_without_removing() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);

    setup::run(
        root,
        &cfg,
        None,
        SetupOptions {
            no_sync: true,
            ..Default::default()
        },
    )
    .unwrap();

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Claude]),
        SetupOptions {
            no_sync: true,
            check: true,
            prune: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(out.exit(), ExitCode::Drift);
    assert_managed_link(&root.join(".copilot/mcp-config.json"));
}
