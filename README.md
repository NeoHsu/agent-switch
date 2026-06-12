# Agent Switch

Agent Switch is a zero-runtime-dependency Rust CLI for synchronizing a canonical `.agents/`
directory with native coding-agent formats.

The binary name is `ags`.

For maintainers, see [`docs/architecture.md`](docs/architecture.md) for the
workspace layout, sync pipeline, manifest semantics, and extension points.

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
--quiet
```

Supported tools in v1:

```text
claude, codex, copilot, opencode, pi, antigravity
```

## Project Config

Agent Switch v1 reads `.agent-switch.yaml` by default. It does not read
`scripts/mappings.yaml`.

```yaml
version: 1
agents_dir: .agents
manifest: .agents/.sync-manifest.json
```

Run `ags init` to create the default config, canonical directories, sample files,
and recommended `.gitignore` entries. Use `ags init --tools codex,copilot` to
write a starter config filtered to only those tool mappings.

Config paths are validated before any setup or sync work runs:

- paths must be repository-relative;
- paths must use forward slashes for portability;
- absolute paths and `.` / `..` path components are rejected;
- `generate` output directories must be unique;
- use either `tool` or `tools` on a mapping, not both;
- `tools` lists must be non-empty and contain no duplicates.

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

## Cross-Platform Behavior

Agent Switch is designed to run on Linux, macOS, and Windows.

- Repository paths in output are normalized to forward slashes.
- Text files are read as UTF-8 and tolerate a leading UTF-8 BOM.
- Markdown frontmatter parsing tolerates CRLF line endings.
- Unix platforms create symlinks for managed links.
- Windows tries to create symlinks; directory links can fall back to junctions.
- If a Windows file symlink cannot be created, Agent Switch falls back to a
  managed plain-file copy so the tool remains usable without Developer Mode or
  administrator privileges.
- Generated files, manifests, and config writes use atomic replacement to reduce
  the chance of partially written files.

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

The workspace declares a minimum supported Rust version (MSRV) of Rust 1.85.

```bash
cargo build --release -p agent-switch-cli
```

The release binary is:

```text
target/release/ags
```

Release builds in CI use explicit target triples for Linux, macOS, and Windows
so archive names match the binaries they contain.

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

CI runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
an MSRV `cargo check --workspace --all-targets`, `cargo audit`, and
`cargo test --workspace` on Linux, macOS, and Windows. Tag pushes matching `v*`
build release archives for Linux, macOS, and Windows.
