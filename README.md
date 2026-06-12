# Agent Switch

Agent Switch is a zero-runtime-dependency Rust CLI for synchronizing a canonical `.agents/`
directory with native coding-agent formats.

The binary name is `ags`.

## Commands

```bash
ags init
ags setup
ags setup --tool codex --prune
ags setup --check
ags sync
ags sync --check
# import-only / export-only are mutually exclusive
ags sync --tool codex,copilot
ags sync --check --import-only
ags sync --check --export-only
ags sync --json
ags sync --json --event-filter generated,merged
ags doctor
ags mappings validate
ags version
```

Global options:

```bash
--root <path>
--config <path>
--tool <list>
--target <list>
--quiet
```

Supported tools in v1:

```text
claude, codex, copilot, opencode, pi, antigravity
```

## Project Config

Agent Switch v1 reads `.agent-switch.yaml` by default. It can still read an
existing `.agentstitch.yaml` as a compatibility fallback, but new repos should use
`.agent-switch.yaml`. It does not read `scripts/mappings.yaml`.

```yaml
version: 1
agents_dir: .agents
manifest: .agents/.sync-manifest.json
```

Run `ags init` to create the default config, canonical directories, sample files,
and recommended `.gitignore` entries.

Symlinks can be declared as a simple `link: target` mapping or as an object with
explicit tool ownership. Custom links without inferred or explicit ownership are
kept when `--tool ... --prune` is used.

```yaml
symlinks:
  CUSTOM.md:
    to: .agents/custom.md
    tools: [codex]
```

Use `ags setup --tool <tool> --prune` when switching tools and you want
Agent Switch to remove links/copy fallbacks for tools that are no longer selected. Pruning
is conservative: unmanaged real files and directories are skipped.

## Migration From Repo-Local Scripts

1. Add `.agent-switch.yaml` to the repo.
2. Keep canonical files under `.agents/`.
3. Run `ags sync --check` in CI to detect drift.
4. Replace repo-local wrapper scripts with:

```bash
ags setup
ags sync
```

The Rust CLI v1 intentionally does not provide an automatic
`scripts/mappings.yaml` to `.agent-switch.yaml` migration command.

## Build

```bash
cargo build --release -p agent-switch-cli
```

The release binary is:

```text
target/release/ags
```

## Sync Event Filtering and JSON Output

`ags sync --event-filter` lets you keep only selected events in text or JSON output.

```bash
ags sync --json --event-filter imported,generated
ags sync --check --json --event-filter drift,synced_no_changes
```

When `--json` is used, events are emitted in a deterministic order and payload
fields are fixed for scripts and CI machines.

## Test

```bash
cargo test
```

CI runs `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo audit`,
and `cargo test` on Linux, macOS, and Windows. Tag pushes matching `v*` build
release archives for Linux, macOS, and Windows.
