use std::path::Path;

use agent_switch_core::tool::Format;

#[test]
fn codex_agent_round_trips() {
    let canonical = "---\nname: reviewer\ndescription: Reviews code changes.\ncodex:\n  sandbox_mode: read-only\n---\nReview the diff.\n";

    let exported = Format::CodexAgent.export(canonical).unwrap();
    assert!(exported.contains("name = \"reviewer\""));
    assert!(exported.contains("sandbox_mode = \"read-only\""));
    assert!(exported.contains("developer_instructions = \"Review the diff.\""));

    let imported = Format::CodexAgent
        .import(Path::new(".codex/agents/reviewer.toml"), &exported)
        .unwrap();
    assert_eq!(imported, canonical);
}

#[test]
fn codex_agent_export_keeps_numeric_values() {
    let canonical = "---\nname: tuner\ndescription: Tunes settings.\ncodex:\n  temperature: 0.5\n  limits:\n  - 1\n  - 2.5\n---\nBody.\n";

    let exported = Format::CodexAgent.export(canonical).unwrap();
    assert!(exported.contains("temperature = 0.5"));
    assert!(exported.contains("2.5"));
}

#[test]
fn codex_agent_preserves_nested_namespace_values() {
    let canonical = r#"---
name: nested
description: Preserves nested values.
codex:
  profile:
    enabled: true
    thresholds:
    - 1
    - 2.5
  modes:
  - name: fast
    enabled: true
---
Body.
"#;

    let exported = Format::CodexAgent.export(canonical).unwrap();
    assert!(exported.contains("[profile]"));
    assert!(exported.contains("thresholds = [1, 2.5]"));
    assert!(exported.contains("modes = [{ name = \"fast\", enabled = true }]"));

    let imported = Format::CodexAgent
        .import(Path::new(".codex/agents/nested.toml"), &exported)
        .unwrap();
    assert!(imported.contains("profile:"));
    assert!(imported.contains("enabled: true"));
    assert!(imported.contains("thresholds:"));
    assert!(imported.contains("2.5"));
    assert!(imported.contains("modes:"));
    assert!(imported.contains("name: fast"));
}

#[test]
fn codex_agent_requires_name_description_and_body() {
    let missing_name = "---\ndescription: Reviews code.\n---\nReview.\n";
    let missing_description = "---\nname: reviewer\n---\nReview.\n";
    let empty_body = "---\nname: reviewer\ndescription: Reviews code.\n---\n";

    let err = Format::CodexAgent.export(missing_name).unwrap_err();
    assert!(err.to_string().contains("codex-agent requires `name`"));
    let err = Format::CodexAgent.export(missing_description).unwrap_err();
    assert!(
        err.to_string()
            .contains("codex-agent requires `description`")
    );
    let err = Format::CodexAgent.export(empty_body).unwrap_err();
    assert!(
        err.to_string()
            .contains("codex-agent requires non-empty developer instructions")
    );
}

#[test]
fn copilot_instructions_round_trip_paths() {
    let canonical =
        "---\ndescription: Unit testing rules.\npaths:\n- src/**/*.rs\n---\nWrite focused tests.\n";

    let exported = Format::CopilotInstructions.export(canonical).unwrap();
    assert!(exported.contains("applyTo:"));
    assert!(exported.contains("src/**/*.rs"));

    let imported = Format::CopilotInstructions
        .import(
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

    let exported = Format::CopilotAgent.export(canonical).unwrap();
    assert!(exported.contains("infer: false"));

    let imported = Format::CopilotAgent
        .import(Path::new(".github/agents/fixer.agent.md"), &exported)
        .unwrap();
    assert_eq!(imported, canonical);
}

#[test]
fn copilot_agent_import_infers_dotted_name_from_native_filename() {
    let native = "---\ndescription: Plans specs.\n---\nPlan the work.\n";

    let imported = Format::CopilotAgent
        .import(
            Path::new(".github/agents/speckit.git.commit.agent.md"),
            native,
        )
        .unwrap();

    assert!(imported.contains("name: speckit.git.commit"));
    assert!(imported.contains("description: Plans specs."));
}

#[test]
fn copilot_prompt_import_infers_dotted_name_from_native_filename() {
    let native = "---\ndescription: Creates tasks.\n---\nCreate tasks.\n";

    let imported = Format::CopilotPrompt
        .import(Path::new(".github/prompts/speckit.tasks.prompt.md"), native)
        .unwrap();

    assert!(imported.contains("name: speckit.tasks"));
    assert!(imported.contains("description: Creates tasks."));
}

#[test]
fn copilot_agent_requires_name_and_description() {
    let missing_name = "---\ndescription: Fixes issues.\n---\nFix it.\n";
    let missing_description = "---\nname: fixer\n---\nFix it.\n";

    let err = Format::CopilotAgent.export(missing_name).unwrap_err();
    assert!(err.to_string().contains("copilot-agent requires `name`"));
    let err = Format::CopilotAgent
        .export(missing_description)
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("copilot-agent requires `description`")
    );
}

#[test]
fn opencode_agent_export_sets_mode_and_import_restores_name() {
    let canonical = "---\ndescription: Reviews code.\n---\nReview.\n";

    let exported = Format::OpencodeAgent.export(canonical).unwrap();
    assert!(exported.contains("mode: subagent"));

    let imported = Format::OpencodeAgent
        .import(Path::new(".opencode/agents/reviewer.md"), &exported)
        .unwrap();
    assert!(imported.contains("name: reviewer"));
    assert!(!imported.contains("mode: subagent"));
}

#[test]
fn opencode_agent_round_trips_namespace() {
    let canonical = "---\nname: reviewer\ndescription: Reviews code.\nopencode:\n  model: anthropic/claude-sonnet-4-6\n---\nReview.\n";

    let exported = Format::OpencodeAgent.export(canonical).unwrap();
    assert!(exported.contains("mode: subagent"));
    assert!(exported.contains("model: anthropic/claude-sonnet-4-6"));

    let imported = Format::OpencodeAgent
        .import(Path::new(".opencode/agents/reviewer.md"), &exported)
        .unwrap();
    assert_eq!(imported, canonical);
}
