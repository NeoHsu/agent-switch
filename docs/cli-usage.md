# CLI Usage Guide

`ags` keeps a canonical `.agents/` directory synchronized with the native
formats used by coding-agent tools.

## Command Surface Review

The current command surface is small enough to keep as-is:

- `init` is for first-time repository bootstrapping.
- `setup` is for preparing native tool entry points and optionally pruning old
  managed links.
- `sync` is for converting between canonical `.agents/` files and native tool
  formats.
- `doctor` is for diagnostics and drift checks.
- `mappings validate` is for config validation in CI or preflight scripts.
- `version` is for release/build metadata.

No command is currently dead code. `mappings validate` overlaps with the
validation that runs before `setup` and `sync`, but it is still useful when a
script wants to validate configuration without touching generated files.
`version` overlaps with `ags --version`, but the subcommand includes commit,
target, and build-date metadata.

## Global Options

These options are accepted globally:

- `--root <path>` or `AGENT_SWITCH_ROOT`: run against a specific repository
  root instead of discovering one from the current directory.
- `--config <path>` or `AGENT_SWITCH_CONFIG`: use a non-default config path.
  This is meaningful for `setup`, `sync`, `doctor`, and `mappings validate`.
- `--tool <list>` or `AGENT_SWITCH_TOOLS`: target a comma-separated tool list.
  This is meaningful for `setup` and `sync`.
- `--quiet`: suppress normal output while preserving exit status.

Because these are global clap options, `--config` and `--tool` appear in help
for commands that do not use them. They are kept global so scripts can pass
common options consistently before or after subcommands.

## First-Time Setup

Create the default config, sample canonical files, directories, and `.gitignore`
entries:

```bash
ags init
```

Create only the starter mappings for selected tools:

```bash
ags init --tools codex,copilot
```

Overwrite starter files and config when regenerating a scratch repository:

```bash
ags init --force
```

`init --tools` is intentionally separate from global `--tool`: it changes the
starter config that gets written. Global `--tool` is a runtime filter for
`setup` and `sync`.

## Preparing Native Tool Files

Create or repair symlinks/copy fallbacks, then run a normal sync:

```bash
ags setup
```

Prepare only one or more tools:

```bash
ags setup --tool codex
ags setup --tool claude,copilot
```

Remove managed links/copy fallbacks for tools that are no longer selected:

```bash
ags setup --tool codex --prune
```

Check what setup would change without writing:

```bash
ags setup --check
ags setup --tool codex --prune --check
```

Only repair links/copies and skip generated-file sync:

```bash
ags setup --no-sync
```

Repair incorrect managed symlinks while still preserving real files and
directories:

```bash
ags setup --force
```

## Synchronizing Files

Run the full pipeline:

```bash
ags sync
```

Full sync stages are:

1. import changed native generated files into canonical `.agents/` files
2. export canonical files to native generated files
3. remove stale generated files tracked by the manifest
4. copy managed link fallbacks when symlinks are unavailable
5. merge canonical MCP config into native config files

Check drift without writing:

```bash
ags sync --check
```

Run only one direction:

```bash
ags sync --import-only
ags sync --export-only
```

Target selected tools:

```bash
ags sync --tool codex,copilot
```

Emit machine-readable output for CI:

```bash
ags sync --json
ags sync --check --json
```

Filter noisy events in text or JSON output:

```bash
ags sync --event-filter generated,merged
ags sync --json --event-filter drift,synced_no_changes
```

`--import-only` and `--export-only` are mutually exclusive. `--check` can be
combined with either one to test one direction without writing files.

## Diagnostics and Validation

Inspect repository health:

```bash
ags doctor
ags doctor --json
```

Validate config mappings without setup or sync side effects:

```bash
ags mappings validate
ags mappings validate --json
```

Use a non-default config:

```bash
ags --config configs/agent-switch.yaml doctor
ags --config configs/agent-switch.yaml mappings validate
```

## Version Metadata

Print human-readable build metadata:

```bash
ags version
```

Print JSON metadata for release automation:

```bash
ags version --json
```

Use `ags --version` only when the package version string is enough.

## CI Patterns

Recommended drift check:

```bash
ags sync --check
```

Recommended machine-readable drift check:

```bash
ags sync --check --json --event-filter drift,synced_no_changes
```

Recommended config preflight:

```bash
ags mappings validate
```

Recommended tool-specific setup validation:

```bash
ags setup --tool codex --prune --check
```

## Exit Codes

`ags` uses structured exit codes from the core library:

- `0`: success
- `1`: drift; `--check` detected changes that would be written
- `2`: config error; invalid config, invalid option combination, unknown tool, or
  similar user-fixable input
- `3`: I/O error; unexpected filesystem or process failure
- `4`: unsupported error; unsupported config version

Scripts should prefer exit status over parsing human-readable output. Use JSON
output when a script needs details.
