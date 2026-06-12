use std::{fs, path::Path};

use agent_switch_core::{
    config::{self, Config, SymlinkDetail, SymlinkSpec},
    init,
    manifest::{self, Manifest},
    setup::{self, SetupOptions},
    tool::Tool,
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
    assert!(!root.join(".agentstitch.yaml").exists());
    assert!(out
        .lines
        .iter()
        .any(|line| line == "created  .agent-switch.yaml"));
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
    assert!(root.join(".copilot/mcp-config.json").is_symlink());
    assert!(root.join(".pi/mcp.json").is_symlink());
    assert!(root.join(".claude/skills").is_symlink());

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

    assert!(out
        .lines
        .iter()
        .any(|line| line == "removed: .copilot/mcp-config.json"));
    assert!(out.lines.iter().any(|line| line == "removed: .pi/mcp.json"));
    assert!(!root.join(".copilot/mcp-config.json").exists());
    assert!(!root.join(".pi/mcp.json").exists());
    assert!(root.join(".claude/skills").is_symlink());
    assert!(root.join(".mcp.json").is_symlink());
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
    assert!(root.join("CUSTOM.md").is_symlink());

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

    assert!(root.join("CUSTOM.md").is_symlink());
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
    assert!(root.join("CUSTOM.md").is_symlink());

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
    assert!(!root.join("CUSTOM.md").exists());
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

    assert!(out
        .lines
        .iter()
        .any(|line| line == "removed: .copilot/mcp-config.json"));
    assert!(!root.join(".copilot/mcp-config.json").exists());
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
    assert!(root.join(".copilot/mcp-config.json").is_symlink());
}
