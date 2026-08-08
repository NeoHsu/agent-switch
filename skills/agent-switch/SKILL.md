---
name: agent-switch
description: >-
  Safely manage a repository's canonical .agents files and synchronize them with
  Claude, Codex, Copilot, OpenCode, Pi, and Antigravity native layouts. Use this
  skill whenever a user asks to migrate, initialize, set up, sync, validate,
  diagnose, prune, or check drift in coding-agent instructions, skills,
  commands, rules, or MCP configuration, including requests such as 「同步
  agent 設定」、「整理 .agents」、「遷移 Claude/Codex/Copilot 設定」 or
  「檢查 agent 設定漂移」.
compatibility: Requires ags CLI 0.2.1 exactly
---

# Agent Switch

`ags` is the deterministic filesystem boundary for a repository whose canonical
agent source is `.agents/`. It creates native links, generates native-format
adapters, merges MCP configuration, and records ownership in a sync manifest.

## Compatibility gate

Before any repository operation, verify the installed CLI against this Skill:

```bash
ags doctor --skill-version 0.2.1 --json
```

Continue only when `skillCompatibility.compatible` is `true`. A mismatch means
the Skill and CLI are from different releases; stop before `init`, `migrate`,
`setup`, or `sync` and install matching versions. This gate does not read the
repository config or mutate files.

Discover the current command safety metadata when choosing an operation:

```bash
ags operation list --json
```

## Choose the workflow

Use `migrate` when native agent files already exist or the repository has been
using native tool layouts first:

```bash
ags --root <repo> migrate --check
ags --root <repo> migrate
```

Use `init` only for a new canonical-first repository that has no native files to
import:

```bash
ags --root <repo> init
ags --root <repo> setup
```

Use `setup` to create or repair native links/copy fallbacks. Use `sync` to
export canonical files, import explicitly requested native edits, remove stale
managed outputs, and merge MCP settings:

```bash
ags --root <repo> setup --check
ags --root <repo> sync --check --export-only --json
ags --root <repo> sync --export-only
```

Use `doctor` and mapping validation for read-only diagnosis:

```bash
ags --root <repo> doctor --json
ags --root <repo> mappings validate --json
```

## Safety rules

- Run a matching `--check` command before a mutating command when reviewing an
  existing repository.
- Treat `setup --prune` and `--force` as destructive: use them only when the
  user explicitly requests removal or overwrite, and prefer the check form first.
- Never overwrite an unmanaged real file or directory to make a link work.
  Reconcile the conflict with the user instead.
- Keep `.agents/` as the source of truth. Do not hand-edit generated files when
  the canonical source can be changed instead.
- Use `--tool <comma-separated-list>` to limit scope. Do not infer a tool from
  a path when an explicit filter is available.
- Do not delete `.agent-switch.lock`, `.agent-switch.operation.json`, or the
  sync manifest to bypass a safety check. Use the recovery hint from `doctor` or
  `sync --reset-manifest` when appropriate.
- For large repositories, bound read-only checks with `--max-files`,
  `--max-source-bytes`, `--max-output-bytes`, and `--max-events`.
  `--max-output-bytes` and `--max-events` require `--check`.

## Automation output

Use JSON output when another tool will consume the result:

```bash
ags --root <repo> sync --check --export-only --json --max-files 500 --max-source-bytes 1048576 --max-output-bytes 1048576 --max-events 1000
ags --root <repo> doctor --json
ags operation list --json
ags schema print cli-output-v1
```

Validate the response against the bundled schema before depending on
command-specific fields. Normal JSON is written to stdout; verbose diagnostics
and requested runtime-error details are written to stderr. Use exit status as
the primary signal: `0` is healthy, `1` is drift, `2` is invalid input/config,
`3` is an I/O failure, and `4` is an unsupported config or runtime version.

## Configuration boundaries

`.agent-switch.yaml` controls mappings, ownership, sync mode, and manifest path.
Paths must be repository-relative and use forward slashes. Unknown config fields,
path traversal, duplicate generated destinations, ambiguous tool ownership, and
link-to-target self-mappings are rejected before filesystem work begins.

Managed state is tracked in `.agents/.sync-manifest.json`. A manifest hash is
not permission to overwrite user content: modified generated files and unmanaged
real files must remain visible as drift.

## Maintainer workflow

When changing Agent Switch itself, read `docs/architecture.md` and
`docs/development.md`, then run the smallest relevant task followed by:

```bash
mise run check:pr
```

Do not weaken a quality or security gate to make a change pass. Update the CLI,
schema, Skill, examples, and documentation together when a public contract
changes.
