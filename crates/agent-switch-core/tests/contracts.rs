use std::fs;

use agent_switch_core::{diagnostics, fs as repo_fs, init, output};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn machine_output_is_versioned_without_changing_command_fields() {
    let rendered = output::render_json(&json!({"valid": true})).unwrap();
    let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();

    assert_eq!(value["schemaVersion"], 1);
    assert_eq!(value["valid"], true);
}

#[test]
fn checked_in_schema_matches_renderer_version() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../schema/cli-output-v1.json")).unwrap();

    assert_eq!(schema["properties"]["schemaVersion"]["const"], 1);
    assert_eq!(schema["required"][0], "schemaVersion");
    assert_eq!(
        schema["properties"]["operation_journal"]["properties"]["present"]["type"],
        "boolean"
    );
    assert_eq!(
        schema["properties"]["operation_journal_error"]["type"],
        json!(["string", "null"])
    );
}

#[test]
fn repository_lock_is_released_for_the_next_invocation() {
    let temp = tempdir().unwrap();
    let lock_path = temp.path().join(repo_fs::REPOSITORY_LOCK_FILE);

    let first = repo_fs::RepositoryLock::acquire(temp.path()).unwrap();
    assert!(lock_path.is_file());
    drop(first);

    let _second = repo_fs::RepositoryLock::acquire(temp.path()).unwrap();
}

#[test]
fn repository_operation_recovers_and_clears_interrupted_state() {
    let temp = tempdir().unwrap();
    let journal_path = temp.path().join(repo_fs::REPOSITORY_OPERATION_FILE);

    let first = repo_fs::RepositoryOperation::begin(temp.path(), "sync").unwrap();
    assert!(first.recovered().is_none());
    drop(first);
    assert!(journal_path.is_file());

    let second = repo_fs::RepositoryOperation::begin(temp.path(), "setup").unwrap();
    assert_eq!(
        second.recovered().map(|record| record.command.as_str()),
        Some("sync")
    );
    second.complete().unwrap();
    assert!(!journal_path.exists());
}

#[test]
fn repository_operation_rejects_non_regular_journal_path() {
    let temp = tempdir().unwrap();
    fs::create_dir(temp.path().join(repo_fs::REPOSITORY_OPERATION_FILE)).unwrap();

    let error = repo_fs::RepositoryOperation::begin(temp.path(), "sync").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unsafe repository operation journal path")
    );
}

#[test]
fn doctor_reports_interrupted_operation_journal() {
    let temp = tempdir().unwrap();
    let operation = repo_fs::RepositoryOperation::begin(temp.path(), "sync").unwrap();
    drop(operation);

    let out = diagnostics::doctor_at(
        temp.path(),
        None,
        &temp.path().join(".agent-switch.yaml"),
        true,
    )
    .unwrap();
    let report: serde_json::Value = serde_json::from_str(&out.lines[0]).unwrap();

    assert_eq!(report["operation_journal"]["present"], true);
    assert_eq!(report["operation_journal"]["command"], "sync");
    assert_eq!(report["drift"], true);
}

#[test]
fn doctor_config_error_reports_malformed_operation_journal() {
    let temp = tempdir().unwrap();
    fs::write(
        temp.path().join(repo_fs::REPOSITORY_OPERATION_FILE),
        "{not json\n",
    )
    .unwrap();

    let config_error = anyhow::anyhow!("invalid config");
    let out = diagnostics::doctor_config_error(
        temp.path(),
        &temp.path().join(".agent-switch.yaml"),
        &config_error,
        true,
    )
    .unwrap();
    let report: serde_json::Value = serde_json::from_str(&out.lines[0]).unwrap();

    assert_eq!(report["operation_journal"]["present"], false);
    assert!(
        report["operation_journal_error"]
            .as_str()
            .is_some_and(|error| error.contains("operation journal is not parseable"))
    );
}

#[test]
fn init_gitignore_tracks_rebuildable_lock_state() {
    let temp = tempdir().unwrap();
    init::run(temp.path(), None, false).unwrap();

    let gitignore = fs::read_to_string(temp.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains(repo_fs::REPOSITORY_LOCK_FILE));
    assert!(gitignore.contains(repo_fs::REPOSITORY_OPERATION_FILE));
}
