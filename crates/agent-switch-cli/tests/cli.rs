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
    assert!(report["config_error"]
        .as_str()
        .unwrap()
        .contains("unsupported config version: 999"));
}
