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
ags sync --tool codex,copilot
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
--verbose
--quiet
--no-color
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

## Test

```bash
cargo test
```
