# CLI Usage Guide

`ags` keeps a canonical `.agents/` directory synchronized with the native
formats used by coding-agent tools.

## Command Surface Review

The current command surface is small enough to keep as-is:

- `init` is for greenfield canonical-first bootstrapping when there are no
  native agent files to import yet.
- `migrate` is the recommended onboarding path for repositories that already
  have native tool files, or teams that prefer to use coding-agent tools first
  and consolidate them later.
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
  This is meaningful for `migrate`, `setup`, `sync`, `doctor`, and
  `mappings validate`.
- `--tool <list>` or `AGENT_SWITCH_TOOLS`: target a comma-separated tool list.
  This is meaningful for `migrate`, `setup`, and `sync`.
- `--quiet`: suppress normal output while preserving exit status.
- `--verbose` or `-v`: print command diagnostics to stderr.
- `--debug`: print detailed diagnostics to stderr; implies `--verbose`.

Because these are global clap options, `--config` and `--tool` appear in help
for commands that do not use them. They are kept global so scripts can pass
common options consistently before or after subcommands.

Verbose and debug diagnostics are written to stderr, so JSON stdout remains
machine-readable.

## Choosing `init` vs `migrate`

Use `migrate` as the normal onboarding path when a repository already has native
coding-agent files, or when developers first use those tools and later decide to
standardize on `.agents/`.

Use `init` only when the repository has no native agent files yet and you want to
author `.agents/` first.

```text
Repo has native agent files?
  examples: .claude/, .codex/, .github/agents/, .opencode/, CLAUDE.md
        |
   +----+----+
   |         |
  yes        no
   |         |
   v         v
ags migrate  Want to create .agents/ now?
             |
        +----+----+
        |         |
       yes        no
        |         |
        v         v
     ags init   no ags command yet
```

## Canonical-First Initialization

Create the default config, sample canonical files, directories, and `.gitignore`
entries for a new repo that has no native files to import:

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
`migrate`, `setup`, and `sync`.

## Migrating Existing Native Tool Files

Import existing Claude, Codex, Copilot, OpenCode, Pi, and Antigravity native
files into the canonical layout, back up managed native paths, then run setup.
This is the preferred first `ags` command for native-first repositories:

```bash
ags migrate
```

Migrate only selected source tools:

```bash
ags --tool claude,copilot migrate
```

Check what would be imported/backed up without writing:

```bash
ags migrate --check
```

Keep existing native paths in place (equivalent to skipping setup):

```bash
ags migrate --keep-native
ags migrate --no-setup
```

`migrate` creates `.agent-switch.yaml` if needed. For symlink/copy mappings it
copies native files such as `CLAUDE.md`, `.claude/commands`, `.agent/rules`, or
`.opencode/commands` into their canonical targets, then backs up the native
paths as `.bak` so `setup` can create managed links. For generated formats it
imports `.github`, `.codex`, and `.opencode` generated files into `.agents/`.
For MCP configs it imports known native MCP shapes into `.agents/mcp.json`.
Conflicting canonical files are skipped unless `--force` is used. Use
`--keep-native` when you want to preserve native files instead of backing them
up.

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

Rebuild a missing or corrupt manifest from the current working tree:

```bash
ags sync --reset-manifest
ags sync --reset-manifest --check
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

JSON sync events include a deterministic `sequence` field that records event
production order before text/JSON event sorting.

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

## Recovery Cases

If no config file exists, choose the onboarding path that matches the repository:

```bash
# Existing native agent files, or native-first workflow
ags migrate

# No native files yet, and you want a canonical .agents/ skeleton
ags init
```

If the sync manifest is corrupt, rebuild it from the working tree:

```bash
ags sync --reset-manifest
```

`ags doctor` reports the manifest path and the same recovery hint. If an older
script cannot pass `--reset-manifest`, deleting `.agents/.sync-manifest.json`
and then running `ags sync` is equivalent. Permission errors include the
attempted action and path, for example creating a parent directory, creating a
symlink, or replacing a generated file.

## Version Metadata

Print human-readable build metadata:

```bash
ags version
```

Print JSON metadata for release automation:

```bash
ags version --json
```

`ags version` includes package version, git commit, target, rustc version,
Cargo.lock SHA-256, and build date. Use `ags --version` only when the package
version string is enough.

## CI Patterns

Recommended canonical-only drift check when `.agents/` is the source of truth and
native generated files should not be imported back:

```bash
ags sync --check --export-only
```

Recommended full drift check when tool-side generated edits are allowed to import
back into `.agents/`:

```bash
ags sync --check
```

Recommended machine-readable drift check:

```bash
ags sync --check --export-only --json --event-filter drift,synced_no_changes
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
