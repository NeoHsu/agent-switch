use std::path::Path;

use agent_switch_core::{formats, tool::Format};

#[test]
fn codex_agent_round_trips() {
    let canonical = "---\nname: reviewer\ndescription: Reviews code changes.\ncodex:\n  sandbox_mode: read-only\n---\nReview the diff.\n";

    let exported = formats::export(Format::CodexAgent, canonical).unwrap();
    assert!(exported.contains("name = \"reviewer\""));
    assert!(exported.contains("sandbox_mode = \"read-only\""));
    assert!(exported.contains("developer_instructions = \"Review the diff.\""));

    let imported = formats::import(
        Format::CodexAgent,
        Path::new(".codex/agents/reviewer.toml"),
        &exported,
    )
    .unwrap();
    assert_eq!(imported, canonical);
}

#[test]
fn codex_agent_export_keeps_numeric_values() {
    let canonical =
        "---\nname: tuner\ncodex:\n  temperature: 0.5\n  limits:\n  - 1\n  - 2.5\n---\nBody.\n";

    let exported = formats::export(Format::CodexAgent, canonical).unwrap();
    assert!(exported.contains("temperature = 0.5"));
    assert!(exported.contains("2.5"));
}

#[test]
fn copilot_instructions_round_trip_paths() {
    let canonical =
        "---\ndescription: Unit testing rules.\npaths:\n- src/**/*.rs\n---\nWrite focused tests.\n";

    let exported = formats::export(Format::CopilotInstructions, canonical).unwrap();
    assert!(exported.contains("applyTo:"));
    assert!(exported.contains("src/**/*.rs"));

    let imported = formats::import(
        Format::CopilotInstructions,
        Path::new(".github/instructions/unit.instructions.md"),
        &exported,
    )
    .unwrap();
    assert!(imported.contains("paths:"));
    assert!(imported.contains("src/**/*.rs"));
    assert!(imported.contains("Write focused tests."));
}

#[test]
fn copilot_agent_round_trips_namespace() {
    let canonical =
        "---\nname: fixer\ndescription: Fixes issues.\ncopilot:\n  infer: false\n---\nFix it.\n";

    let exported = formats::export(Format::CopilotAgent, canonical).unwrap();
    assert!(exported.contains("infer: false"));

    let imported = formats::import(
        Format::CopilotAgent,
        Path::new(".github/agents/fixer.agent.md"),
        &exported,
    )
    .unwrap();
    assert_eq!(imported, canonical);
}

#[test]
fn opencode_agent_export_sets_mode_and_import_restores_name() {
    let canonical = "---\ndescription: Reviews code.\n---\nReview.\n";

    let exported = formats::export(Format::OpencodeAgent, canonical).unwrap();
    assert!(exported.contains("mode: subagent"));

    let imported = formats::import(
        Format::OpencodeAgent,
        Path::new(".opencode/agents/reviewer.md"),
        &exported,
    )
    .unwrap();
    assert!(imported.contains("name: reviewer"));
    assert!(!imported.contains("mode: subagent"));
}
