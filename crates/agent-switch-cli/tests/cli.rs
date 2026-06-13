use std::{fs, process::Command};

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

    let output = Command::new(env!("CARGO_BIN_EXE_ags"))
        .arg("--root")
        .arg(root)
        .arg("doctor")
        .arg("--json")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(4));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["config"], false);
    assert_eq!(report["config_path"], ".agent-switch.yaml");
    assert!(
        report["config_error"]
            .as_str()
            .unwrap()
            .contains("unsupported config version: 999")
    );
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
