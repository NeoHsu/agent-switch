use std::{fs, path::Path, process::Command};

use agent_switch_core::{
    CommandOutput, ExitCode,
    config::{self, Config, GeneratedTracking, MergeSpec, SymlinkDetail, SymlinkSpec},
    diagnostics, init,
    manifest::{self, Manifest},
    setup::{self, SetupOptions},
    tool::{MergeFormat, Tool},
};
use tempfile::tempdir;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn git_path_is_ignored(root: &Path, path: &str) -> bool {
    let output = Command::new("git")
        .args([
            "-c",
            "core.excludesFile=/dev/null",
            "check-ignore",
            "--quiet",
            "--",
            path,
        ])
        .current_dir(root)
        .output()
        .unwrap();
    match output.status.code() {
        Some(0) => true,
        Some(1) => false,
        code => panic!(
            "git check-ignore failed with {code:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    }
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
    let cfg = config::load_config(root, None).unwrap().0;
    assert_eq!(cfg.agents_dir, Path::new(".agents"));
    assert_eq!(cfg.manifest, Path::new(".agents/.sync-manifest.json"));
    assert!(!cfg.symlinks.contains_key(".agent/rules"));
    assert!(cfg.symlinks.contains_key(".agent/workflows"));
    assert!(!cfg.symlinks.contains_key(".agent/skills"));
    assert!(cfg.symlinks.contains_key(".pi/prompts"));
    assert!(!cfg.symlinks.contains_key(".pi/skills"));
    assert!(!cfg.symlinks.contains_key(".pi/extensions"));
    assert!(!cfg.symlinks.contains_key(".pi/settings.json"));
    assert!(!cfg.symlinks.contains_key(".pi/themes"));
    assert!(cfg.merge.contains_key("antigravity-mcp-config"));
    assert!(!root.join(".agents/pi").exists());
    let starter_skill =
        fs::read_to_string(root.join(".agents/skills/example-skill/SKILL.md")).unwrap();
    assert!(starter_skill.contains("name: example-skill"));
    assert!(starter_skill.contains("description: Example placeholder skill."));
    assert!(
        out.lines
            .iter()
            .any(|line| line == "created  .agent-switch.yaml")
    );
    let config_text = fs::read_to_string(root.join(".agent-switch.yaml")).unwrap();
    assert!(!config_text.contains('\\'));
    let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(gitignore.contains(".agents/.sync-manifest.json"));
    assert!(!gitignore.contains("\n.agents/\n"));
    assert!(gitignore.contains("\n.agent/workflows\n"));
    assert!(!gitignore.contains("\n.agent/rules\n"));
    assert!(!gitignore.contains("\n.agent/skills\n"));
    assert!(!gitignore.contains("\n.agent/\n"));
    assert!(gitignore.contains("\n.pi/prompts\n"));
    assert!(!gitignore.contains("\n.pi/skills\n"));
    for path in [
        ".pi/",
        ".pi/extensions",
        ".pi/settings.json",
        ".pi/themes",
        ".pi/git/",
        ".pi/npm/",
    ] {
        assert!(!gitignore.contains(&format!("\n{path}\n")));
    }
    assert!(!gitignore.contains(".github/agents/"));
    assert!(!gitignore.contains(".github/prompts/"));
    assert!(!gitignore.contains(".github/instructions/"));
    for path in [
        ".agents/mcp_config.json",
        ".codex/agents/",
        ".codex/config.toml",
        ".opencode/agents/",
        "opencode.json",
    ] {
        assert!(gitignore.contains(&format!("\n{path}\n")));
    }
    for parent in [".claude/", ".codex/", ".copilot/", ".opencode/", ".pi/"] {
        assert!(!gitignore.contains(&format!("\n{parent}\n")));
    }
    assert!(gitignore.contains("\nCLAUDE.md\n"));
    assert!(gitignore.contains("\n.mcp.json\n"));
}

#[test]
fn init_refreshes_existing_gitignore_block() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join(".gitignore"),
        "keep-this\n\n# >>> agent-switch >>>\n.pi/\nold-entry\n# <<< agent-switch <<<\n\nkeep-that\n",
    );

    init::run(root, None, false).unwrap();

    let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(gitignore.contains("keep-this"));
    assert!(gitignore.contains("keep-that"));
    assert!(gitignore.contains("\n.agent/workflows\n"));
    assert!(!gitignore.contains("\n.agent/rules\n"));
    assert!(!gitignore.contains("\n.agent/skills\n"));
    assert!(!gitignore.contains("\n.agent/\n"));
    assert!(gitignore.contains("\n.pi/prompts\n"));
    assert!(!gitignore.contains("\n.pi/skills\n"));
    assert!(!gitignore.contains("\n.pi/extensions\n"));
    assert!(!gitignore.contains("\n.pi/\n"));
    assert!(!gitignore.contains("old-entry"));
}

#[test]
fn for_agents_dir_drops_links_inside_canonical_dir() {
    let cfg = Config::for_agents_dir(".agent".into());

    assert!(!cfg.symlinks.contains_key(".agent/rules"));
    assert!(!cfg.symlinks.contains_key(".agent/workflows"));
    assert!(!cfg.symlinks.contains_key(".agent/skills"));
    assert!(cfg.symlinks.contains_key(".claude/rules"));
}

#[test]
fn init_refreshes_gitignore_from_the_existing_config() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let mut cfg = Config::default();
    cfg.generated_tracking
        .insert("codex-agents".into(), GeneratedTracking::Tracked);
    config::write_config(&root.join(".agent-switch.yaml"), &cfg, false).unwrap();
    write(
        &root.join(".gitignore"),
        "# >>> agent-switch >>>\n.codex/\n# <<< agent-switch <<<\n",
    );

    init::run(root, None, false).unwrap();

    let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(!gitignore.contains("\n.codex/\n"));
    assert!(!gitignore.contains("\n.codex/agents/\n"));
    assert!(gitignore.contains("\n.codex/config.toml\n"));
    let reloaded = config::load_config(root, None).unwrap().0;
    assert_eq!(
        reloaded.generated_tracking.get("codex-agents"),
        Some(&GeneratedTracking::Tracked)
    );
}

#[test]
fn gitignore_honors_generated_tracking_without_parent_directory_rules() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let status = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success());

    let mut cfg = Config::default();
    let mut out = CommandOutput::default();
    init::update_gitignore_for_config(root, &cfg, &mut out).unwrap();
    for path in [
        ".agents/mcp_config.json",
        ".codex/agents/reviewer.toml",
        ".codex/config.toml",
        ".opencode/agents/reviewer.md",
        "opencode.json",
        ".github/agents/reviewer.agent.md",
        ".opencode/commands/build.md",
    ] {
        write(&root.join(path), "generated\n");
    }

    for path in [
        ".agents/mcp_config.json",
        ".codex/agents/reviewer.toml",
        ".codex/config.toml",
        ".opencode/agents/reviewer.md",
        "opencode.json",
    ] {
        assert!(git_path_is_ignored(root, path), "expected ignored: {path}");
    }
    assert!(!git_path_is_ignored(
        root,
        ".github/agents/reviewer.agent.md"
    ));
    assert!(git_path_is_ignored(root, ".opencode/commands/build.md"));

    for id in [
        "antigravity-mcp-config",
        "codex-agents",
        "codex-config",
        "opencode-agents",
        "opencode-config",
    ] {
        cfg.generated_tracking
            .insert(id.into(), GeneratedTracking::Tracked);
    }
    init::update_gitignore_for_config(root, &cfg, &mut out).unwrap();

    for path in [
        ".agents/mcp_config.json",
        ".codex/agents/reviewer.toml",
        ".codex/config.toml",
        ".opencode/agents/reviewer.md",
        "opencode.json",
    ] {
        assert!(
            !git_path_is_ignored(root, path),
            "tracked output is still ignored: {path}"
        );
    }
    assert!(git_path_is_ignored(root, ".opencode/commands/build.md"));
}

#[test]
fn gitignore_does_not_ignore_a_custom_canonical_agent_directory() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = Config::for_agents_dir(".agent".into());
    let mut out = CommandOutput::default();

    init::update_gitignore_for_config(root, &cfg, &mut out).unwrap();

    let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(!gitignore.contains("\n.agent/\n"));
    assert!(gitignore.contains(".agent/.sync-manifest.json"));
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
fn init_with_copilot_uses_workspace_mcp_symlink() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    init::run(root, Some("copilot"), false).unwrap();
    let cfg = config::load_config(root, None).unwrap().0;

    assert!(cfg.symlinks.contains_key(".mcp.json"));
    assert!(cfg.merge.is_empty());

    setup::run(
        root,
        &cfg,
        Some(&[Tool::Copilot]),
        SetupOptions {
            no_sync: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_managed_link(&root.join(".mcp.json"));
    assert_absent(&root.join(".copilot/mcp-config.json"));
}

#[test]
fn init_with_pi_maps_prompts_and_uses_native_skills() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    init::run(root, Some("pi"), false).unwrap();
    let cfg = config::load_config(root, None).unwrap().0;
    write(
        &root.join(".agents/commands/review.md"),
        "Review the current changes.\n",
    );
    write(
        &root.join(".pi/extensions/status.ts"),
        "export default function status() {}\n",
    );
    write(
        &root.join(".pi/themes/contrast.json"),
        "{\"name\":\"contrast\"}\n",
    );
    write(&root.join(".pi/settings.json"), "{\"packages\":[]}\n");

    assert_eq!(
        cfg.symlinks.keys().map(String::as_str).collect::<Vec<_>>(),
        vec![".pi/prompts"]
    );
    assert!(cfg.generate.is_empty());
    assert!(cfg.merge.is_empty());
    assert!(!root.join(".agents/pi").exists());

    setup::run(
        root,
        &cfg,
        Some(&[Tool::Pi]),
        SetupOptions {
            no_sync: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_managed_link(&root.join(".pi/prompts"));
    assert_absent(&root.join(".pi/skills"));
    assert!(root.join(".pi/prompts/review.md").exists());
    assert!(root.join(".agents/skills/example-skill/SKILL.md").exists());
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
        "{\"packages\":[]}\n"
    );
    assert_absent(&root.join(".claude/skills"));
    assert_absent(&root.join(".pi/mcp.json"));
    let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(gitignore.contains("\n.pi/prompts\n"));
    assert!(!gitignore.contains("\n.pi/skills\n"));
    for path in [
        ".pi/",
        ".pi/extensions",
        ".pi/settings.json",
        ".pi/themes",
        ".pi/git/",
        ".pi/npm/",
    ] {
        assert!(!gitignore.contains(&format!("\n{path}\n")));
    }
}

#[test]
fn setup_creates_nested_claude_links_for_nested_agents_files() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write(&root.join("packages/api/AGENTS.md"), "# API agents\n");
    write(
        &root.join(".agents/ignored/AGENTS.md"),
        "# canonical metadata\n",
    );

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Claude]),
        SetupOptions {
            no_sync: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_managed_link(&root.join("packages/api/CLAUDE.md"));
    assert_absent(&root.join(".agents/ignored/CLAUDE.md"));
    assert!(out.lines.iter().any(|line| {
        line.starts_with("created  packages/api/CLAUDE.md -> ") && line.contains("AGENTS.md")
    }));
}

#[test]
fn setup_does_not_create_nested_claude_links_for_other_tool_filters() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write(&root.join("packages/api/AGENTS.md"), "# API agents\n");

    setup::run(
        root,
        &cfg,
        Some(&[Tool::Codex]),
        SetupOptions {
            no_sync: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_absent(&root.join("packages/api/CLAUDE.md"));
}

#[test]
fn setup_prune_removes_nested_claude_links_for_unselected_tools() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write(&root.join("packages/api/AGENTS.md"), "# API agents\n");

    setup::run(
        root,
        &cfg,
        Some(&[Tool::Claude]),
        SetupOptions {
            no_sync: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_managed_link(&root.join("packages/api/CLAUDE.md"));

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Codex]),
        SetupOptions {
            no_sync: true,
            prune: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_absent(&root.join("packages/api/CLAUDE.md"));
    assert!(
        out.lines
            .iter()
            .any(|line| line == "removed: packages/api/CLAUDE.md")
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
fn config_rejects_unknown_fields() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join(".agent-switch.yaml"),
        "version: 1\nsync_mdoe: full\n",
    );

    let err = config::load_config(root, None).unwrap_err();

    assert!(err.to_string().contains("sync_mdoe"));
}

#[test]
fn config_rejects_unknown_mapping_fields() {
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
    recusive: true
"#,
    );

    let err = config::load_config(root, None).unwrap_err();

    assert!(err.to_string().contains("recusive"));
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

    let out =
        diagnostics::doctor_at(root, Some(&cfg), &root.join(".agent-switch.yaml"), false).unwrap();

    assert!(
        out.lines.iter().any(|line| {
            line == "warning: manifest is not parseable: .agents/.sync-manifest.json"
        })
    );
    assert!(
        out.lines
            .iter()
            .any(|line| line == "hint:    run `ags sync --reset-manifest` to rebuild it")
    );
}

#[test]
fn doctor_json_reports_manifest_recovery_hint() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write(&root.join(".agents/.sync-manifest.json"), "{not json\n");

    let out =
        diagnostics::doctor_at(root, Some(&cfg), &root.join(".agent-switch.yaml"), true).unwrap();
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
        Some("Run `ags sync --reset-manifest` to rebuild it.")
    );
}

#[test]
fn doctor_reports_missing_manifest_as_drift() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);

    let out =
        diagnostics::doctor_at(root, Some(&cfg), &root.join(".agent-switch.yaml"), false).unwrap();

    assert_eq!(out.exit(), ExitCode::Drift);
    assert!(
        out.lines
            .iter()
            .any(|line| line == "warning: manifest is missing: .agents/.sync-manifest.json")
    );
}

#[test]
fn doctor_json_uses_custom_config_and_reports_unmanaged_links() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let config_path = root.join("configs/custom.yaml");
    write(
        &config_path,
        "version: 1\nsymlinks:\n  CLAUDE.md: AGENTS.md\n",
    );
    fs::create_dir_all(root.join(".agents")).unwrap();
    write(&root.join("AGENTS.md"), "# Canonical\n");
    write(&root.join("CLAUDE.md"), "# Unmanaged\n");
    let cfg = config::load_config(root, Some(Path::new("configs/custom.yaml")))
        .unwrap()
        .0;

    let out = diagnostics::doctor_at(root, Some(&cfg), &config_path, true).unwrap();
    let report: serde_json::Value = serde_json::from_str(&out.lines[0]).unwrap();

    assert_eq!(out.exit(), ExitCode::Drift);
    assert_eq!(report["config"], true);
    assert_eq!(report["config_path"], "configs/custom.yaml");
    assert_eq!(report["manifest"], false);
    assert_eq!(report["manifest_exists"], false);
    assert_eq!(report["links"][0]["status"], "unmanaged");
    assert_eq!(report["drift"], true);
}

#[cfg(unix)]
#[test]
fn doctor_reports_wrong_symlink_target() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join(".agent-switch.yaml"),
        "version: 1\nsymlinks:\n  CLAUDE.md: AGENTS.md\n",
    );
    fs::create_dir_all(root.join(".agents")).unwrap();
    write(&root.join("AGENTS.md"), "# Canonical\n");
    write(&root.join("OTHER.md"), "# Other\n");
    std::os::unix::fs::symlink("OTHER.md", root.join("CLAUDE.md")).unwrap();
    let mut sync_manifest = Manifest::default();
    manifest::save(
        &root.join(".agents/.sync-manifest.json"),
        &mut sync_manifest,
    )
    .unwrap();
    let cfg = config::load_config(root, None).unwrap().0;

    let out = diagnostics::doctor(root, Some(&cfg), true).unwrap();
    let report: serde_json::Value = serde_json::from_str(&out.lines[0]).unwrap();

    assert_eq!(out.exit(), ExitCode::Drift);
    assert_eq!(report["links"][0]["status"], "wrong_target");
}

#[test]
fn setup_reports_missing_canonical_target_without_creating_a_dangling_link() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let mut cfg = fixture(root);
    cfg.symlinks.insert(
        ".pi/missing-resource".into(),
        SymlinkSpec::Target(".agents/missing-resource".into()),
    );

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Pi]),
        SetupOptions {
            no_sync: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(out.exit(), ExitCode::Drift);
    assert!(out.lines.iter().any(|line| {
        line == "skipped  .pi/missing-resource: canonical target is missing: .agents/missing-resource"
    }));
    assert_absent(&root.join(".pi/missing-resource"));
}

#[test]
fn setup_check_reports_existing_real_file_as_drift() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write(&root.join(".pi/prompts"), "real file\n");

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Pi]),
        SetupOptions {
            no_sync: true,
            check: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(out.exit(), ExitCode::Drift);
    assert!(
        out.lines.iter().any(|line| {
            line.starts_with("skipped  .pi/prompts: existing real file or directory")
        })
    );
}

#[test]
fn setup_adopts_identical_existing_files_as_managed_copies() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write(&root.join("CLAUDE.md"), "# Agents\n");

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Claude]),
        SetupOptions {
            no_sync: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(out.exit(), ExitCode::Ok);
    assert!(
        out.lines
            .iter()
            .any(|line| line == "adopted  CLAUDE.md (managed copy)")
    );
    let sync_manifest = manifest::load(&root.join(".agents/.sync-manifest.json")).unwrap();
    assert!(sync_manifest.links.contains_key("CLAUDE.md"));
}

#[test]
fn init_with_antigravity_uses_native_canonical_resources() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    init::run(root, Some("antigravity"), false).unwrap();
    let cfg = config::load_config(root, None).unwrap().0;

    assert_eq!(
        cfg.symlinks.keys().map(String::as_str).collect::<Vec<_>>(),
        vec![".agent/workflows"]
    );
    assert_eq!(
        cfg.merge.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["antigravity-mcp-config"]
    );
    assert!(root.join(".agents/rules/code-style.md").exists());
    assert!(root.join(".agents/skills/example-skill/SKILL.md").exists());

    setup::run(
        root,
        &cfg,
        Some(&[Tool::Antigravity]),
        SetupOptions::default(),
    )
    .unwrap();

    assert_managed_link(&root.join(".agent/workflows"));
    assert_absent(&root.join(".agent/rules"));
    assert_absent(&root.join(".agent/skills"));
    assert!(root.join(".agents/mcp_config.json").exists());
    let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(gitignore.contains("\n.agent/workflows\n"));
    assert!(gitignore.contains("\n.agents/mcp_config.json\n"));
    assert!(!gitignore.contains("\n.agent/rules\n"));
    assert!(!gitignore.contains("\n.agent/skills\n"));
}

#[test]
fn setup_reports_existing_real_file_as_drift() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write(&root.join(".pi/prompts"), "real file\n");

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Pi]),
        SetupOptions {
            no_sync: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(out.exit(), ExitCode::Drift);
}

#[test]
fn setup_check_includes_generated_sync_drift() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write_reviewer_agent(root);

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Codex]),
        SetupOptions {
            check: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(out.exit(), ExitCode::Drift);
    assert!(
        out.lines
            .iter()
            .any(|line| line == "generated: .codex/agents/reviewer.toml")
    );
    assert!(!root.join(".codex/agents/reviewer.toml").exists());
    assert!(!root.join(".codex/config.toml").exists());
}

#[test]
fn setup_prune_removes_unchanged_stale_copy_and_keeps_modified_copy() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write(&root.join(".legacy/unchanged.md"), "unchanged\n");
    write(&root.join(".legacy/modified.md"), "user edit\n");
    let manifest_path = root.join(".agents/.sync-manifest.json");
    let mut tracked = manifest::load(&manifest_path).unwrap();
    tracked.links.insert(
        ".legacy/unchanged.md".into(),
        manifest::sha256_bytes(b"unchanged\n"),
    );
    tracked.links.insert(
        ".legacy/modified.md".into(),
        manifest::sha256_bytes(b"original\n"),
    );
    manifest::save(&manifest_path, &mut tracked).unwrap();

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Codex]),
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
            .any(|line| line == "removed: .legacy/unchanged.md")
    );
    assert_absent(&root.join(".legacy/unchanged.md"));
    assert_eq!(
        fs::read_to_string(root.join(".legacy/modified.md")).unwrap(),
        "user edit\n"
    );
    assert!(out.lines.iter().any(|line| {
        line == "skipped  .legacy/modified.md: managed copy was modified; preserve or merge it manually"
    }));
    assert_eq!(out.exit(), ExitCode::Drift);
    let tracked = manifest::load(&manifest_path).unwrap();
    assert!(!tracked.links.contains_key(".legacy/unchanged.md"));
    assert!(tracked.links.contains_key(".legacy/modified.md"));
}

#[test]
fn setup_prune_rejects_unsafe_stale_manifest_paths() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("repo");
    fs::create_dir(&root).unwrap();
    let cfg = fixture(&root);
    let outside = temp.path().join("outside.md");
    write(&outside, "outside\n");
    let manifest_path = root.join(".agents/.sync-manifest.json");
    let mut tracked = manifest::load(&manifest_path).unwrap();
    tracked
        .links
        .insert("../outside.md".into(), manifest::sha256_bytes(b"outside\n"));
    manifest::save(&manifest_path, &mut tracked).unwrap();

    let out = setup::run(
        &root,
        &cfg,
        Some(&[Tool::Codex]),
        SetupOptions {
            no_sync: true,
            prune: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(fs::read_to_string(outside).unwrap(), "outside\n");
    assert_eq!(out.exit(), ExitCode::Drift);
    assert!(out.lines.iter().any(|line| {
        line == "skipped  ../outside.md: unsafe path in manifest; run `ags sync --reset-manifest`"
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
    assert_managed_link(&root.join(".pi/prompts"));
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

    assert!(out.lines.iter().any(|line| line == "removed: .pi/prompts"));
    assert_absent(&root.join(".pi/prompts"));
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
    fs::create_dir_all(root.join(".pi/prompts")).unwrap();

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

    assert_eq!(out.exit(), ExitCode::Drift);
    assert!(
        out.lines.iter().any(|line| {
            line.starts_with("skipped  .pi/prompts: existing real file or directory")
        })
    );
    assert!(root.join(".pi/prompts").is_dir());
}

#[test]
fn setup_prune_removes_manifest_tracked_copy_fallback() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let mut cfg = fixture(root);
    write(&root.join(".agents/prompt-index.json"), "{}\n");
    write(&root.join(".pi/prompt-index.json"), "{}\n");
    cfg.symlinks.insert(
        ".pi/prompt-index.json".into(),
        SymlinkSpec::Target(".agents/prompt-index.json".into()),
    );
    let mut sync_manifest = Manifest::default();
    sync_manifest.links.insert(
        ".pi/prompt-index.json".into(),
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
            .any(|line| line == "removed: .pi/prompt-index.json")
    );
    assert_absent(&root.join(".pi/prompt-index.json"));
    let next_manifest = manifest::load(&root.join(".agents/.sync-manifest.json")).unwrap();
    assert!(!next_manifest.links.contains_key(".pi/prompt-index.json"));
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
    assert_managed_link(&root.join(".pi/prompts"));
}

fn write_reviewer_agent(root: &Path) {
    write(
        &root.join(".agents/agents/reviewer.md"),
        "---\nname: reviewer\ndescription: Reviews code.\n---\nReview the diff.\n",
    );
}

#[test]
fn setup_prune_removes_generated_outputs_and_merge_targets_for_unselected_tools() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write_reviewer_agent(root);

    setup::run(root, &cfg, None, SetupOptions::default()).unwrap();
    assert!(root.join(".github/agents/reviewer.agent.md").exists());
    assert!(root.join(".opencode/agents/reviewer.md").exists());
    assert!(root.join("opencode.json").exists());
    assert!(root.join(".agents/mcp_config.json").exists());
    assert_managed_link(&root.join(".mcp.json"));
    assert!(root.join(".codex/agents/reviewer.toml").exists());

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Codex]),
        SetupOptions {
            prune: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(
        out.lines
            .iter()
            .any(|line| line == "removed: .github/agents/reviewer.agent.md")
    );
    assert_absent(&root.join(".github/agents/reviewer.agent.md"));
    assert_absent(&root.join(".github/agents"));
    assert_absent(&root.join(".opencode/agents/reviewer.md"));
    assert_absent(&root.join("opencode.json"));
    assert!(
        out.lines
            .iter()
            .any(|line| line == "removed: .agents/mcp_config.json")
    );
    assert_absent(&root.join(".agents/mcp_config.json"));
    assert!(out.lines.iter().any(|line| line == "removed: .mcp.json"));
    assert_absent(&root.join(".mcp.json"));
    assert!(root.join(".codex/agents/reviewer.toml").exists());
    assert!(root.join(".codex/config.toml").exists());

    let tracked = manifest::load(&root.join(".agents/.sync-manifest.json")).unwrap();
    assert!(
        !tracked
            .generated
            .contains_key(".github/agents/reviewer.agent.md")
    );
    assert!(
        tracked
            .generated
            .contains_key(".codex/agents/reviewer.toml")
    );
}

#[test]
fn setup_prune_skips_modified_generated_outputs() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write_reviewer_agent(root);

    setup::run(root, &cfg, None, SetupOptions::default()).unwrap();
    let generated = root.join(".github/agents/reviewer.agent.md");
    let mut edited = fs::read_to_string(&generated).unwrap();
    edited.push_str("\nLocal tweak.\n");
    fs::write(&generated, edited).unwrap();

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Codex]),
        SetupOptions {
            prune: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(out.exit(), ExitCode::Drift);
    assert!(
        out.lines
            .iter()
            .any(|line| { line.starts_with("skipped  .github/agents/reviewer.agent.md:") })
    );
    assert!(generated.exists());
    assert_absent(&root.join(".opencode/agents/reviewer.md"));
}

#[test]
fn setup_prune_removes_legacy_copilot_mcp_output_when_still_managed() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    let mut legacy_cfg = cfg.clone();
    legacy_cfg.merge.insert(
        "copilot-mcp-config".into(),
        MergeSpec {
            to: ".copilot/mcp-config.json".into(),
            format: MergeFormat::Copilot,
            tool: None,
            tools: None,
        },
    );

    setup::run(root, &legacy_cfg, None, SetupOptions::default()).unwrap();
    assert!(root.join(".copilot/mcp-config.json").exists());

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Codex]),
        SetupOptions {
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
    assert_absent(&root.join(".copilot"));
}

#[test]
fn setup_prune_keeps_unmanaged_legacy_copilot_mcp_file() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write(
        &root.join(".copilot/mcp-config.json"),
        "{\"userOwned\":true}\n",
    );

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Codex]),
        SetupOptions {
            no_sync: true,
            prune: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(out.exit(), ExitCode::Drift);
    assert_eq!(
        fs::read_to_string(root.join(".copilot/mcp-config.json")).unwrap(),
        "{\"userOwned\":true}\n"
    );
    assert!(out.lines.iter().any(|line| {
        line == "skipped  .copilot/mcp-config.json: not recognized as agent-switch output; remove it manually"
    }));
}

#[test]
fn setup_prune_cleans_codex_marker_block_preserving_user_content() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let cfg = fixture(root);
    write_reviewer_agent(root);
    write(
        &root.join(".agents/mcp.json"),
        "{\n  \"mcpServers\": {\n    \"demo\": {\"command\": \"npx\"}\n  }\n}\n",
    );
    write(&root.join(".codex/config.toml"), "theme = \"dark\"\n");

    setup::run(root, &cfg, None, SetupOptions::default()).unwrap();
    let merged = fs::read_to_string(root.join(".codex/config.toml")).unwrap();
    assert!(merged.contains("# >>> agent-switch:mcp >>>"));
    assert!(merged.contains("theme = \"dark\""));

    let out = setup::run(
        root,
        &cfg,
        Some(&[Tool::Claude]),
        SetupOptions {
            prune: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(
        out.lines
            .iter()
            .any(|line| { line.starts_with("cleaned  .codex/config.toml:") })
    );
    let cleaned = fs::read_to_string(root.join(".codex/config.toml")).unwrap();
    assert!(!cleaned.contains("# >>> agent-switch:mcp >>>"));
    assert!(cleaned.contains("theme = \"dark\""));
    assert_absent(&root.join(".codex/agents/reviewer.toml"));
}
