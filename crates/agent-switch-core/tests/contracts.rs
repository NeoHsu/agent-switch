use std::fs;

use agent_switch_core::{fs as repo_fs, init, output};
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
fn repository_lock_is_released_for_the_next_invocation() {
    let temp = tempdir().unwrap();
    let lock_path = temp.path().join(repo_fs::REPOSITORY_LOCK_FILE);

    let first = repo_fs::RepositoryLock::acquire(temp.path()).unwrap();
    assert!(lock_path.is_file());
    drop(first);

    let _second = repo_fs::RepositoryLock::acquire(temp.path()).unwrap();
}

#[test]
fn init_gitignore_tracks_rebuildable_lock_state() {
    let temp = tempdir().unwrap();
    init::run(temp.path(), None, false).unwrap();

    let gitignore = fs::read_to_string(temp.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains(repo_fs::REPOSITORY_LOCK_FILE));
}
