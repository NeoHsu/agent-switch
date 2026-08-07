use std::{fs, process::Command};

use agent_switch_core::fs::REPOSITORY_OPERATION_FILE;
use tempfile::tempdir;

#[test]
fn doctor_rejects_invalid_config() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join(".agent-switch.yaml"), "version: 999\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("--root")
        .arg(root)
        .arg("doctor")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(4));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("unsupported config version: 999"),
        "stdout was: {stdout}"
    );
}

#[test]
fn doctor_json_reports_invalid_config() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join(".agent-switch.yaml"), "version: 999\n").unwrap();
    fs::write(root.join(REPOSITORY_OPERATION_FILE), "{not json\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("--root")
        .arg(root)
        .arg("doctor")
        .arg("--json")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(4));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["config"], false);
    assert_eq!(report["config_path"], ".agent-switch.yaml");
    assert!(
        report["config_error"]
            .as_str()
            .unwrap()
            .contains("unsupported config version: 999")
    );
    assert!(
        report["operation_journal_error"]
            .as_str()
            .is_some_and(|error| error.contains("operation journal is not parseable"))
    );
    assert_eq!(
        fs::read_to_string(root.join(REPOSITORY_OPERATION_FILE)).unwrap(),
        "{not json\n"
    );
}

#[test]
fn migrate_claude_project_from_cli() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("CLAUDE.md"), "# Claude\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("--root")
        .arg(root)
        .arg("--tool")
        .arg("claude")
        .arg("migrate")
        .arg("--no-setup")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(root.join("AGENTS.md")).unwrap(),
        "# Claude\n"
    );
    assert!(root.join("CLAUDE.md.bak").exists());
}

#[test]
fn sync_reset_manifest_rebuilds_corrupt_manifest() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    let init = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("--root")
        .arg(root)
        .arg("init")
        .output()
        .unwrap();
    assert_eq!(init.status.code(), Some(0));

    let manifest_path = root.join(".agents/.sync-manifest.json");
    fs::write(&manifest_path, "{not json\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("--root")
        .arg(root)
        .arg("sync")
        .arg("--reset-manifest")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("warning: reset manifest: rebuilding .agents/.sync-manifest.json"),
        "stdout was: {stdout}"
    );
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["meta"]["tool"], "agent-switch");
}

#[test]
fn verbose_sync_json_keeps_stdout_machine_readable() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    let init = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("--root")
        .arg(root)
        .arg("init")
        .output()
        .unwrap();
    assert_eq!(init.status.code(), Some(0));

    let output = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("--root")
        .arg(root)
        .arg("--verbose")
        .arg("sync")
        .arg("--json")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["exit"], "Ok");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("verbose: command: sync"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains("verbose: sync stages: export, remove-stale, sync-links, merge"),
        "stderr was: {stderr}"
    );
}

#[test]
fn read_only_check_does_not_create_operation_journal() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    let init = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("--root")
        .arg(root)
        .arg("init")
        .output()
        .unwrap();
    assert_eq!(init.status.code(), Some(0));

    let check = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("--root")
        .arg(root)
        .arg("sync")
        .arg("--check")
        .output()
        .unwrap();

    assert!(check.status.code().is_some_and(|code| code <= 1));
    assert!(!root.join(REPOSITORY_OPERATION_FILE).exists());
}

#[test]
fn interrupted_mutation_is_recovered_on_next_successful_mutation() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join(".agent-switch.yaml"), "version: 999\n").unwrap();

    let failed = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("--root")
        .arg(root)
        .arg("sync")
        .output()
        .unwrap();

    assert_eq!(failed.status.code(), Some(4));
    assert!(root.join(REPOSITORY_OPERATION_FILE).is_file());

    let doctor = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("--root")
        .arg(root)
        .arg("doctor")
        .arg("--json")
        .output()
        .unwrap();
    let doctor_report: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(doctor_report["operation_journal"]["present"], true);
    assert_eq!(doctor_report["operation_journal"]["command"], "sync");

    fs::remove_file(root.join(".agent-switch.yaml")).unwrap();
    let recovered = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("--root")
        .arg(root)
        .arg("init")
        .arg("--force")
        .output()
        .unwrap();

    assert_eq!(recovered.status.code(), Some(0));
    assert!(!root.join(REPOSITORY_OPERATION_FILE).exists());
    let stderr = String::from_utf8_lossy(&recovered.stderr);
    assert!(
        stderr.contains("warning: recovered interrupted sync operation"),
        "stderr was: {stderr}"
    );
}

#[test]
fn help_uses_plural_canonical_directory() {
    let output = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("--help")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("canonical .agents files"));
    assert!(stdout.contains("containing .agent-switch.yaml, .agents, or .git"));
}

#[test]
fn version_json_reports_build_metadata() {
    let output = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("version")
        .arg("--json")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        report["rustc"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert_eq!(report["schemaVersion"], 1);
    assert!(
        report["cargo_lock_sha256"]
            .as_str()
            .is_some_and(|value| value.len() == 64)
    );
}

#[test]
fn json_runtime_errors_use_versioned_stderr_envelope() {
    let temp = tempdir().unwrap();
    fs::write(temp.path().join(".agent-switch.yaml"), "version: 999\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("--root")
        .arg(temp.path())
        .arg("sync")
        .arg("--json")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(4));
    let report: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["error"]["kind"], "unsupported");
    assert!(
        report["error"]["message"]
            .as_str()
            .is_some_and(|message| { message.contains("unsupported config version: 999") })
    );
}
