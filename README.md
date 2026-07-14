# Agent Switch

Agent Switch is a zero-runtime-dependency Rust CLI for synchronizing a canonical `.agents/`
directory with native coding-agent formats.

The binary name is `ags`.

For maintainers, see [`docs/architecture.md`](docs/architecture.md) for the
workspace layout, sync pipeline, manifest semantics, and extension points.
For CLI workflows and option semantics, see [`docs/cli-usage.md`](docs/cli-usage.md).
For canonical `.agents/` file conventions and frontmatter examples, see
[`docs/canonical-files.md`](docs/canonical-files.md).

## Install

Download a prebuilt archive for Linux, macOS, or Windows from the
[latest GitHub release](https://github.com/NeoHsu/agent-switch/releases/latest),
or build from source with Rust 1.85 or newer:

```bash
cargo build --release -p agent-switch-cli
install -m 0755 target/release/ags ~/.local/bin/ags
```

## Quickstart

For most existing repositories, let coding-agent tools create their native files
first, then consolidate those files into the canonical `.agents/` layout:

```bash
ags migrate --check
ags migrate
# or only selected source tools
ags --tool claude,copilot migrate
```

Use `init` only for a new canonical-first repository that has no native agent
files to import yet:

```bash
ags init
ags setup
ags sync --export-only
```

Check for drift in CI without writing files:

```bash
ags sync --check --export-only
```

Switch to a selected tool set and remove old managed links, file-copy
fallbacks, generated outputs, and managed MCP merge content for tools that are
no longer selected:

```bash
ags setup --tool codex --prune
ags sync --tool codex
```

## Commands

```bash
# native-first onboarding
ags migrate
ags migrate --check

# canonical-first bootstrap for repos with no native files yet
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

See [`docs/cli-usage.md`](docs/cli-usage.md) for the full scenario guide,
including bootstrap, tool switching, drift checks, JSON output, and CI usage.

Global options:

```bash
--root <path>
--config <path>
--tool <list>
--quiet
--verbose
--debug
```

Supported tools in v1:

```text
claude, codex, copilot, opencode, pi, antigravity
```

Default integrations by tool:

| Tool | Native paths managed by default | Canonical source | Integration mode |
| --- | --- | --- | --- |
| Claude | `.claude/agents`, `.claude/commands`, `.claude/rules`, `.claude/skills`, `CLAUDE.md`, `.mcp.json` | `.agents/agents`, `.agents/commands`, `.agents/rules`, `.agents/skills`, `AGENTS.md`, `.agents/mcp.json` | symlink/copy; no generated adapter |
| Codex | native `AGENTS.md` and `.agents/skills`; `.codex/agents/*.toml`, `.codex/config.toml` | `AGENTS.md`, `.agents/skills`, `.agents/agents/*.md`, `.agents/mcp.json` | direct context/skills discovery; generated TOML agents; MCP marker-block merge |
| Copilot | native `AGENTS.md` and `.agents/skills`; `.github/agents/*.agent.md`, `.github/prompts/*.prompt.md`, `.github/instructions/**/*.instructions.md`, `.mcp.json` | `AGENTS.md`, `.agents/skills`, `.agents/agents`, `.agents/commands`, `.agents/rules`, `.agents/mcp.json` | direct context/skills discovery; generated Markdown; shared MCP symlink/copy |
| OpenCode | native `AGENTS.md` and `.agents/skills`; `.opencode/commands`, `.opencode/agents/*.md`, `opencode.json` | `AGENTS.md`, `.agents/skills`, `.agents/commands`, `.agents/agents`, `.agents/mcp.json` | direct context/skills discovery; commands link; generated agents; MCP merge |
| Pi | native `AGENTS.md` and `.agents/skills`; `.pi/prompts` | `AGENTS.md`, `.agents/skills`, `.agents/commands` | direct context/skills discovery; prompt-template link; no built-in subagents or MCP |
| Antigravity | native `AGENTS.md`, `.agents/rules`, `.agents/skills`, and `.agents/mcp_config.json`; `.agent/workflows` compatibility path | `AGENTS.md`, `.agents/rules`, `.agents/skills`, `.agents/mcp.json`, `.agents/commands` | direct context/rules/skills discovery; native MCP conversion; workflow link |

Pi reads root/ancestor `AGENTS.md` and `.agents/skills` directly. Agent Switch
only exposes shared commands at Pi's native `.pi/prompts` path. Official Pi
does not ship built-in subagents, a path-scoped rules directory, or
MCP, so those are not fabricated by the default integration. Pi-only resources
such as extensions, themes, project settings, `SYSTEM.md`, and
`APPEND_SYSTEM.md` remain project-owned and trackable under `.pi/`.

Existing `.agent-switch.yaml` files are never silently rewritten on upgrade. To
adopt these Pi defaults, keep/add `.pi/prompts`, remove `.pi/skills` and the
legacy `.pi/mcp.json` mapping, then run `ags --tool pi migrate`; Pi discovers
canonical skills directly. For current Antigravity, remove `.agent/rules` and
`.agent/skills` before `ags --tool antigravity migrate`; `.agents/rules` and
`.agents/skills` are now its preferred paths, and MCP is rendered to
`.agents/mcp_config.json`. Use `ags init --force --tools antigravity` (or another
explicit tool list) only when replacing the entire existing config is
intentional. Existing Copilot configs should likewise add
`.mcp.json: .agents/mcp.json` and remove the legacy `copilot-mcp-config` merge
job; `ags --tool copilot migrate` imports and
backs up the obsolete `.copilot/mcp-config.json`, while Copilot discovers the
shared workspace file directly.

For a path-by-path canonical-to-native matrix, see
[`docs/canonical-files.md`](docs/canonical-files.md#default-integration-map).
For `symlink/copy` entries, setup creates managed links; on Windows, directory
link failures can use junctions and file symlink failures can use managed
plain-file copies.

## Project Config

Agent Switch v1 reads `.agent-switch.yaml` by default. It does not read
`scripts/mappings.yaml`.

```yaml
version: 1
agents_dir: .agents
manifest: .agents/.sync-manifest.json
```

Default config generated by `ags init` or by `ags migrate` when no config exists
contains managed-link mappings, generation mappings, and MCP merge mappings
similar to:

```yaml
version: 1
agents_dir: .agents
manifest: .agents/.sync-manifest.json
sync_mode: canonical-only

generated_tracking:
  copilot-agents: tracked
  copilot-prompts: tracked
  copilot-instructions: tracked
  claude: ignored
  codex-agents: ignored
  opencode-agents: ignored
  antigravity-mcp-config: ignored

symlinks:
  .claude/skills: .agents/skills
  .claude/agents: .agents/agents
  .claude/commands: .agents/commands
  .claude/rules: .agents/rules
  .opencode/commands: .agents/commands
  .pi/prompts: .agents/commands
  .agent/workflows: .agents/commands
  .mcp.json: .agents/mcp.json
  CLAUDE.md: AGENTS.md

generate:
  copilot-agents:
    from: .agents/agents
    to: .github/agents
    format: copilot-agent
    suffix: .agent.md
  copilot-prompts:
    from: .agents/commands
    to: .github/prompts
    format: copilot-prompt
    suffix: .prompt.md
  copilot-instructions:
    from: .agents/rules
    to: .github/instructions
    format: copilot-instructions
    suffix: .instructions.md
    recursive: true
  opencode-agents:
    from: .agents/agents
    to: .opencode/agents
    format: opencode-agent
    suffix: .md
  codex-agents:
    from: .agents/agents
    to: .codex/agents
    format: codex-agent
    suffix: .toml

merge:
  opencode-config:
    to: opencode.json
    format: opencode
  codex-config:
    to: .codex/config.toml
    format: codex
  antigravity-mcp-config:
    to: .agents/mcp_config.json
    format: antigravity
```

Run `ags migrate` when a repository already has native agent files, or when your
team prefers to use coding-agent tools first and consolidate them later. It
creates `.agent-switch.yaml` if needed, imports supported native layouts into
`.agents/`, backs up managed native paths as `.bak`, and then runs setup. Use
`ags init` only for a new canonical-first repository with no native files to
import; it creates the default config, canonical directories, sample files, and
recommended `.gitignore` entries. `ags init --tools codex,copilot` writes a
starter config filtered to only those tool mappings.

For Claude compatibility, `ags setup` also discovers nested repository
instructions named `AGENTS.md` and creates same-directory managed `CLAUDE.md`
links or copy fallbacks. For example, `packages/api/AGENTS.md` produces
`packages/api/CLAUDE.md`. Hidden tool output directories such as `.agents/`,
`.claude/`, `.github/`, and `.git/` are skipped.

`sync_mode: canonical-only` makes plain `ags sync` export from `.agents/` to
native adapters without importing native edits back into the canonical tree.
Use `ags sync --import-only` when you explicitly want to pull managed generated
edits back into canonical files.

`generated_tracking` controls `.gitignore` generation by mapping id. Ignore
rules are emitted for exact managed paths rather than whole tool directories,
so changing a mapping to `tracked` makes that output committable without also
unignoring unrelated adapters. Copilot generated files default to `tracked`
because GitHub-hosted Copilot reads `.github/agents`, `.github/prompts`, and
`.github/instructions` from the repository. Re-run `ags init` (without
`--force`) or `ags migrate` after editing tracking settings to refresh the
managed `.gitignore` block from the active config.

Config paths are validated before any setup or sync work runs:

- paths must be repository-relative;
- paths must use forward slashes for portability;
- absolute paths and `.` / `..` path components are rejected;
- `generate` output directories must be unique;
- unknown or misspelled config fields are rejected;
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
Agent Switch to remove everything it manages for tools that are no longer
selected: managed links, file-copy fallbacks, generated outputs (for example
`.github/agents/*.agent.md`), and managed MCP merge content (`opencode.json`'s
`mcp` object, the `.codex/config.toml` marker block, and
`.agents/mcp_config.json`'s `mcpServers` object). Prune also removes an obsolete
`.copilot/mcp-config.json` only when it exactly matches generated legacy output.
Pruning is conservative: unmanaged real files, modified managed-copy
fallbacks, modified generated outputs, and directories with user content are
skipped and reported instead of deleted. An unresolved setup or prune operation
exits with the drift code instead of reporting success.

During `ags sync`, managed file copies are only reconciled when they are
tracked in the sync manifest. A real file sitting at a managed link location
that Agent Switch never created is left untouched and reported as a warning;
it is never copied over the canonical source.

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

## Migrating Native Tool Files

Import native files into `.agents/`:

```bash
ags --tool claude,copilot migrate
ags doctor
ags sync --export-only
```

Migration preserves dotted Copilot filenames such as
`speckit.git.commit.agent.md` as `.agents/agents/speckit.git.commit.md`, infers
missing `name` fields from native filenames, strips native suffixes, and prefers
full Copilot instructions over same-named Claude pointer rules.

## Migration From Repo-Local Scripts

1. Add `.agent-switch.yaml` to the repo.
2. Keep canonical files under `.agents/`.
3. Run `ags sync --check` in CI to detect drift.
4. Replace repo-local wrapper scripts with:

```bash
ags setup
ags sync
```

The Rust CLI v1 migrates native tool files, but it does not parse arbitrary
repo-local wrapper scripts.

## Build

The workspace declares a minimum supported Rust version (MSRV) of Rust 1.85.

```bash
cargo build --release -p agent-switch-cli
```

The release binary is:

```text
target/release/ags
```

Install the release binary somewhere on your `PATH`, for example:

```bash
cargo build --release -p agent-switch-cli
install -m 0755 target/release/ags ~/.local/bin/ags
```

Release builds in CI use explicit target triples for Linux, macOS, and Windows
so archive names match the binaries they contain.

## Exit Codes

| Code | Meaning |
| --- | --- |
| 0 | command succeeded |
| 1 | drift detected by `--check`/`doctor`, or setup/prune could not complete |
| 2 | invalid config or command input |
| 3 | I/O or unexpected runtime error |
| 4 | unsupported config version or platform behavior |

## Sync Event Filtering and JSON Output

`ags sync --event-filter <events>` keeps only selected events in text or JSON
output.

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

CI runs `cargo fmt --all --check`, Clippy with warnings denied, an MSRV
`cargo check --workspace --all-targets --locked`, `cargo audit --deny warnings`,
and `cargo test --workspace --locked` on Linux, macOS, and Windows. Tag pushes
matching `v*` repeat format, lint, test, and audit verification before building
release archives for Linux, macOS, and Windows.

## License

Agent Switch is available under the [MIT License](LICENSE).
