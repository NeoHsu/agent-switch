# CLI Usage Guide

`ags` keeps a canonical `.agents/` directory synchronized with the native
formats used by coding-agent tools. For the default coding-agent path mapping,
see [Canonical `.agents/` Files](canonical-files.md#default-integration-map).

## Command Overview

The command surface is:

- `init` is for greenfield canonical-first bootstrapping when there are no
  native agent files to import yet.
- `migrate` is the recommended onboarding path for repositories that already
  have native tool files, or teams that prefer to use coding-agent tools first
  and consolidate them later.
- `setup` is for preparing native tool entry points and optionally pruning old
  managed links.
- `sync` is for converting between canonical `.agents/` files and native tool
  formats.
- `doctor` is for diagnostics, drift checks, and the fail-closed CLI/Skill version gate.
- `mappings validate` is for config validation in CI or preflight scripts.
- `operation list` exposes stable operation IDs and safety metadata for agents.
- `schema list` and `schema print` expose bundled machine-readable contracts.
- `version` is for release/build metadata.

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

## Contract discovery

List the operation catalog in human-readable or machine-readable form:

```bash
ags operation list
ags operation list --json
```

The catalog reports stable IDs, read/write/destructive risk, whether an operation
mutates repository files, and whether it supports JSON output. Use it to choose
safe commands instead of inferring side effects from command names.

Discover and print bundled schemas:

```bash
ags schema list --json
ags schema print cli-output-v1
ags schema print cli-output-v1.schema.json
```

`schema print` emits the exact schema document without adding a response envelope;
`schema list --json` is itself a versioned CLI response.

Verbose and debug diagnostics are written to stderr, so JSON stdout remains
machine-readable. Every JSON object response includes additive `schemaVersion: 1`;
the command-specific fields remain stable. The shared contract is versioned in
[`schema/cli-output-v1.json`](../schema/cli-output-v1.json). Runtime failures before a command
report are rendered as versioned error objects on stderr; diagnostic reports stay
on stdout.

Mutating commands acquire the per-repository `.agent-switch.lock` before dispatch
so concurrent `ags` processes cannot interleave manifest or generated-file writes. They
also leave a temporary `.agent-switch.operation.json` journal while running; a later
mutation reports an interrupted operation if the previous process exited before cleanup.
`--check`, `doctor`, and mapping validation paths remain read-only and do not create a
journal.

Verify the exact CLI/Skill pairing without reading repository configuration:

```bash
ags doctor --skill-version 0.2.1 --json
```

Continue only when `skillCompatibility.compatible` is `true`. A mismatch exits
with code `2` and should be fixed by installing the matching Skill and CLI
release.

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

Keep existing native paths in place and skip the final setup/sync pass:

```bash
ags migrate --keep-native
```

Import and back up native paths, but skip the final setup/sync pass:

```bash
ags migrate --no-setup
```

`migrate` creates `.agent-switch.yaml` if needed. For managed-link mappings it
copies native files such as `CLAUDE.md`, `.mcp.json`, `.claude/commands`,
`.claude/rules`, `.opencode/commands`, or `.pi/prompts` into their canonical
targets, then backs up the native paths as `.bak` so `setup` can create managed
links, Windows directory junctions, or file-copy fallbacks as needed. For
generated formats it imports `.github`, `.codex`, and `.opencode` generated
files into `.agents/`.
For MCP configs it imports known native MCP shapes—including Antigravity's
`.agents/mcp_config.json` and legacy `.copilot/mcp-config.json`—into
`.agents/mcp.json`; the obsolete Copilot file is then backed up. Official Pi has
no MCP config.
Pi migration covers `.pi/prompts` and imports legacy `.pi/skills` content into
the canonical directory; current Pi reads `AGENTS.md` and `.agents/skills`
directly. Pi-only extensions, themes, and
project settings remain unmanaged.
Conflicting canonical files are skipped unless `--force` is used. Use
`--keep-native` when you want to preserve native files instead of backing them
up. Migration also preserves dotted Copilot agent and prompt names, infers
missing `name` fields from native filenames, strips native suffixes such as
`.agent.md` and `.prompt.md`, and prefers full Copilot instructions over
same-named Claude pointer rules.

## Preparing Native Tool Files

Create or repair managed links, Windows junctions, or file-copy fallbacks, then
run a normal sync:

```bash
ags setup
```

When Pi is selected, setup exposes shared commands at `.pi/prompts`. Pi reads
`AGENTS.md` and `.agents/skills` directly, so setup does not duplicate skills at
`.pi/skills`. Project-local resources load after Pi project trust is approved.
Pi-only extensions, themes, settings, and package state remain under `.pi/`
without Agent Switch ownership.

When Claude is selected, setup also discovers nested `AGENTS.md` files and
creates managed same-directory `CLAUDE.md` links or copy fallbacks. For example,
`packages/api/AGENTS.md` becomes `packages/api/CLAUDE.md`. Tool output and
hidden management directories such as `.agents/`, `.claude/`, `.github/`, and
`.git/` are skipped.

Prepare only one or more tools:

```bash
ags setup --tool codex
ags setup --tool claude,copilot
```

Remove everything Agent Switch manages for tools that are no longer selected —
managed links, file-copy fallbacks, generated outputs, and managed MCP merge
content (the `mcp` object in `opencode.json`, the `.codex/config.toml` marker
block, and the `mcpServers` object in `.agents/mcp_config.json`). Unmanaged real
files, modified managed-copy fallbacks, and modified generated outputs are
skipped and reported:

```bash
ags setup --tool codex --prune
```

Check what setup would change without writing. Unless `--no-sync` is also set,
this includes the same generated-file, copy-fallback, and MCP drift check that a
normal setup would run:

```bash
ags setup --check
ags setup --tool codex --prune --check
```

Unmanaged paths or modified outputs that prevent setup or prune from completing
are reported and return exit code `1` rather than silently succeeding.

Only repair links/fallbacks and skip generated-file sync:

```bash
ags setup --no-sync
```

Repair incorrect managed symlinks while still preserving real files and
directories:

```bash
ags setup --force
```

## Synchronizing Files

Export canonical files to native adapters:

```bash
ags sync
```

The default generated config uses `sync_mode: canonical-only`, so plain
`ags sync` runs these export-side stages:

1. export canonical files to native generated files
2. remove stale generated files tracked by the manifest
3. copy managed link fallbacks when symlinks are unavailable
4. merge canonical MCP config into native config files

Set `sync_mode: full` or pass `--import-only` when you explicitly want to pull
managed native edits back into canonical `.agents/` files.

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
production order before text/JSON event sorting. The response also includes
`schemaVersion: 1` for automation contract negotiation.

Filter noisy events in text or JSON output:

```bash
ags sync --event-filter generated,merged
ags sync --json --event-filter drift,synced_no_changes
```

Bound read-only drift checks when running against a large repository:

```bash
ags sync --check --export-only --json --max-files 500 --max-source-bytes 1048576 --max-output-bytes 1048576 --max-events 1000
```

`--max-files` and `--max-source-bytes` bound plan construction. The output and
event limits are check-only safeguards so a failed budget cannot interrupt a
mutating sync halfway through its stages.

`--import-only` and `--export-only` are mutually exclusive. `--check` can be
combined with either one to test one direction without writing files.

## Diagnostics and Validation

Inspect repository health:

```bash
ags doctor
ags doctor --json
```

`doctor` validates the resolved config path, canonical directory, manifest
existence and parseability, managed-link targets, copy-fallback state, generated
outputs, and MCP drift. Health drift returns exit code `1`. JSON output includes
`links`, `generated_files_in_sync`, and a top-level `drift` boolean.

Validate config mappings without setup or sync side effects. Unknown or
misspelled fields are rejected instead of silently falling back to defaults:

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
and then running `ags sync` is equivalent. If a process is interrupted during a
mutation, rerun the same or another mutating command; the stale operation journal is
recognized under the repository lock and reported as a warning. `ags doctor --json`
exposes the same state in `operation_journal` (`present`, `command`, `pid`, and
`started_at_unix_secs`). If an invalid config and an unreadable or malformed
journal are present together, `operation_journal_error` preserves the secondary
diagnostic without hiding the config error. Permission errors include the attempted
action and path, for example creating a parent directory,
creating a symlink, or replacing a generated file.

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
- `1`: drift; `--check` or `doctor` detected an unhealthy or incomplete state
- `2`: config error; invalid config, invalid option combination, unknown tool, or
  similar user-fixable input
- `3`: I/O error; unexpected filesystem or process failure
- `4`: unsupported error; unsupported config version

Scripts should prefer exit status over parsing human-readable output. Use JSON
output when a script needs details.
