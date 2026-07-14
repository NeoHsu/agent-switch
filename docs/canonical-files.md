# Canonical `.agents/` Files

Agent Switch treats `.agents/` as the repository-owned source of truth and
converts those files into each supported tool's native layout.

## Directory Layout

```text
.agents/
├── agents/       reusable subagents
├── commands/     command and prompt definitions
├── rules/        coding rules and instructions
├── skills/       skill folders
└── mcp.json      canonical MCP server config
```

`ags migrate` creates this layout by importing existing native agent files.
`ags init` creates starter directories and sample files only for new
canonical-first repositories with no native files to import. Teams should commit
the canonical files. Generated native outputs can be tracked or ignored per
adapter; GitHub Copilot outputs default to tracked because GitHub-hosted
Copilot reads `.github/**` from the repository.

Repository-local `AGENTS.md` files are also supported for Claude compatibility.
Root `AGENTS.md` maps to root `CLAUDE.md` through the default config, and
`ags setup --tool claude` discovers nested files such as
`packages/api/AGENTS.md` and creates same-directory managed `CLAUDE.md` links or
copy fallbacks.

## Default Integration Map

The default `.agent-switch.yaml` maps canonical files to tool-native paths as
follows. `link/copy` means `ags setup` creates managed links. On Windows,
directory link failures can fall back to junctions, and file symlink failures
can fall back to managed plain-file copies. Those file-copy fallbacks are
tracked in the sync manifest and reconciled by `ags sync`; a real file at a
managed link location that Agent Switch never created is reported as a warning
and never copied over the canonical source. `generated` and `merged` paths are
written by `ags sync`.

<!-- markdownlint-disable MD013 -->

| Canonical source | Claude | Codex | Copilot | OpenCode | Pi | Antigravity |
| --- | --- | --- | --- | --- | --- | --- |
| `AGENTS.md` | `CLAUDE.md` link/copy | native read | native read | native read | native read | native read |
| `.agents/agents/*.md` | `.claude/agents` link/copy | `.codex/agents/*.toml` generated | `.github/agents/*.agent.md` generated | `.opencode/agents/*.md` generated | — | — |
| `.agents/commands/*.md` | `.claude/commands` link/copy | — | `.github/prompts/*.prompt.md` generated | `.opencode/commands` link/copy | `.pi/prompts` link/copy | `.agent/workflows` link/copy |
| `.agents/rules/**/*.md` | `.claude/rules` link/copy | — | `.github/instructions/**/*.instructions.md` generated recursively | — | — | native read |
| `.agents/skills/*` | `.claude/skills` link/copy | native read | native read | native read | native read | native read |
| `.agents/mcp.json` | `.mcp.json` link/copy | `.codex/config.toml` merged marker block | `.mcp.json` link/copy | `opencode.json` merged | — | `.agents/mcp_config.json` merged |

Tool-level view:

| Tool | Native paths managed by default | Canonical source | Notes |
| --- | --- | --- | --- |
| Claude | `.claude/agents`, `.claude/commands`, `.claude/rules`, `.claude/skills`, `CLAUDE.md`, `.mcp.json` | `.agents/agents`, `.agents/commands`, `.agents/rules`, `.agents/skills`, `AGENTS.md`, `.agents/mcp.json` | Direct managed-link integration; edits through real symlinks or Windows junctions affect canonical files immediately. |
| Codex | native `AGENTS.md` and `.agents/skills`; `.codex/agents/*.toml`, `.codex/config.toml` | `AGENTS.md`, `.agents/skills`, `.agents/agents/*.md`, `.agents/mcp.json` | Context and skills are read directly; agents are exported as TOML; MCP uses a marker block. |
| Copilot | native `AGENTS.md` and `.agents/skills`; `.github/agents/*.agent.md`, `.github/prompts/*.prompt.md`, `.github/instructions/**/*.instructions.md`, `.mcp.json` | `AGENTS.md`, `.agents/skills`, `.agents/agents`, `.agents/commands`, `.agents/rules`, `.agents/mcp.json` | Context and skills are read directly; generated Markdown covers agents/prompts/instructions; MCP uses the shared workspace file. |
| OpenCode | native `AGENTS.md` and `.agents/skills`; `.opencode/commands`, `.opencode/agents/*.md`, `opencode.json` | `AGENTS.md`, `.agents/skills`, `.agents/commands`, `.agents/agents`, `.agents/mcp.json` | Context and skills are read directly; commands are linked, agents generated, and MCP merged. |
| Pi | native `AGENTS.md` and `.agents/skills`; `.pi/prompts` | `AGENTS.md`, `.agents/skills`, `.agents/commands` | Context and skills are read directly; shared commands use a managed prompt link. Pi-only resources remain unmanaged. |
| Antigravity | native `AGENTS.md`, `.agents/rules`, `.agents/skills`, and `.agents/mcp_config.json`; `.agent/workflows` compatibility path | `AGENTS.md`, `.agents/rules`, `.agents/skills`, `.agents/mcp.json`, `.agents/commands` | Context, rules, and skills are read directly; MCP is converted to the native schema; workflows use the supported compatibility path. |

Sync behavior by integration type:

| Integration type | Written by | Import behavior |
| --- | --- | --- |
| `link/copy` | `ags setup`; Windows file-copy fallbacks are reconciled by `ags sync` | `ags migrate` imports existing native files before replacing them with managed links, junctions, or file-copy fallbacks. Real symlink and junction edits directly update the canonical target. File-copy fallback edits can be copied back during `ags sync`. |
| `generated` | `ags sync` export stage | `ags migrate` imports existing generated files. Later `ags sync` can import tool-side generated edits back into `.agents/` unless `--export-only` is used. |
| `merged` | `ags sync` merge stage | `ags migrate` imports known native MCP shapes into `.agents/mcp.json`. Later sync merges canonical MCP config back to native configs. |

<!-- markdownlint-enable MD013 -->

A dash (`—`) means Agent Switch has no default managed output for that tool and
canonical source type. `native read` means the canonical path is already one of
the tool's discovery locations, so setup must not create a duplicate adapter.
The matrix was checked against Claude Code 2.1.206, Codex CLI 0.143.0, GitHub
Copilot CLI 1.0.70, Pi 0.80.6, OpenCode 1.17.20, Antigravity CLI 1.1.2, and
the current Antigravity 2.0/IDE documentation.

Codex, Copilot, OpenCode, Pi, and current Antigravity releases discover root and
ancestor `AGENTS.md` directly. Claude Code instead discovers `CLAUDE.md`, so it
is the only default context-file adapter. Codex, Copilot, OpenCode, Pi, and
Antigravity consume `.agents/skills` directly, while Antigravity also consumes
`.agents/rules`; duplicating those resources into legacy tool-specific folders
would add drift without improving discovery. Codex only activates project-local
`.codex/config.toml` after the repository is trusted in Codex.

## Pi Integration

Pi project integration (verified with 0.80.6) exposes only shared canonical
resources:

- root and ancestor `AGENTS.md` files directly as project context;
- `.agents/skills/**/SKILL.md` directly through Pi's Agent Skills discovery;
- `.agents/commands/*.md` through `.pi/prompts`, where the filename becomes the
  Pi slash-command name.

Project-local Pi resources require project trust. Run `/trust` and restart Pi,
or use `pi --approve` for a one-off non-interactive invocation. Prompt discovery
is non-recursive, matching the default non-recursive canonical commands layout.
`ags migrate --tool pi` imports native prompts and any legacy `.pi/skills`
content before setup; current Pi then reads the canonical skills directly.

Pi intentionally does not include built-in subagents, a path-scoped rules
folder, or MCP. Agent Switch therefore does not reinterpret `.agents/agents` or
`.agents/rules` as another resource type, and it does not create a default Pi
MCP file. Pi-only resources—including `.pi/extensions`, `.pi/themes`,
`.pi/settings.json`, `.pi/SYSTEM.md`, and `.pi/APPEND_SYSTEM.md`—remain
project-owned, unmanaged, and trackable.

## Agents

Canonical agents are Markdown files under `.agents/agents/`. Universally mapped
fields such as `name` and `description` stay at the top level. Tool-specific
fields live in a namespace such as `copilot`, `opencode`, or `codex`.
Top-level `tools` and `model` remain useful for direct-link consumers such as
Claude, but generated adapters do not translate them automatically because each
tool uses different names, values, and schemas. Put generated-adapter settings
in that adapter's namespace.

```markdown
---
name: reviewer
description: Reviews code changes.
tools: Read, Grep
model: sonnet
copilot:
  infer: false
opencode:
  model: anthropic/claude-sonnet-4-6
codex:
  sandbox_mode: read-only
---
Review the diff and call out correctness, regression, and test risks.
```

Generated outputs:

| Tool | Output |
| --- | --- |
| Copilot | `.github/agents/reviewer.agent.md` |
| OpenCode | `.opencode/agents/reviewer.md` |
| Codex | `.codex/agents/reviewer.toml` |

Validation:

| Target format | Required canonical fields |
| --- | --- |
| `copilot-agent` | non-empty `name`, non-empty `description` |
| `codex-agent` | `name`, `description`, and Markdown body must be non-empty |

Tool-specific generated edits can be imported back into the canonical file. If
both the canonical and generated file changed since the manifest was written,
the current import behavior chooses the tool-side content for that tool. Fields
outside that tool's namespace—including unknown custom fields and future tool
namespaces—remain preserved.

## Commands and Prompts

Canonical commands are Markdown files under `.agents/commands/`.

```markdown
---
name: fix
description: Fix an issue.
copilot:
  agent: agent
---
Fix the issue described by the user and keep the patch focused.
```

Default mappings use this content in two ways:

| Tool | Behavior |
| --- | --- |
| Claude | `.claude/commands` links or copies `.agents/commands` |
| OpenCode | `.opencode/commands` links or copies `.agents/commands` |
| Pi | managed `.pi/prompts`; filenames become slash commands |
| Antigravity | `.agent/workflows` links or copies `.agents/commands` |
| Copilot | exports `.github/prompts/*.prompt.md` |

## Rules and Instructions

Canonical rules are Markdown files under `.agents/rules/`. Nested rule files are
supported by the default Copilot instructions mapping.

```markdown
---
description: Unit testing rules.
paths:
- "src/**/*.rs"
- "tests/**/*.rs"
---
Write focused tests for behavior changed by the patch.
```

For Copilot instructions, `paths` is exported as the native `applyTo` field and
imported back to `paths`. If `paths` is missing or empty, Agent Switch exports
`applyTo: "**"`.

Default mappings use rules in these ways:

| Tool | Behavior |
| --- | --- |
| Claude | `.claude/rules` links or copies `.agents/rules` |
| Antigravity | reads `.agents/rules` directly |
| Copilot | exports recursive `.github/instructions/*.instructions.md` |

## Skills

Canonical skills live under `.agents/skills/`, typically one directory per skill
with a `SKILL.md` file:

```text
.agents/skills/example-skill/SKILL.md
```

Claude receives a managed `.claude/skills` link/copy because it does not use the
canonical path. Codex, GitHub Copilot, OpenCode, Pi, and Antigravity discover
`.agents/skills` directly, so no generated or linked duplicate is needed for
those tools. Migration still recognizes legacy `.pi/skills` and `.agent/skills`
sources and imports them before retiring those duplicate paths.

## MCP Config

The canonical MCP config is `.agents/mcp.json`:

```json
{
  "mcpServers": {
    "context7": {
      "command": "npx",
      "args": ["-y", "@upstash/context7-mcp"],
      "env": {
        "KEY": "${KEY}"
      }
    }
  }
}
```

Default mappings expose or merge this file as:

| Tool | Behavior |
| --- | --- |
| Claude | `.mcp.json` links or copies `.agents/mcp.json` |
| Copilot | `.mcp.json` links or copies `.agents/mcp.json` |
| OpenCode | merges into `opencode.json` |
| Codex | merges into `.codex/config.toml` |
| Antigravity | merges into `.agents/mcp_config.json`; canonical remote `url` becomes native `serverUrl` |

Explicit legacy configs using `format: copilot` remain accepted for backward
compatibility, but new defaults do not generate `.copilot/mcp-config.json`
because current Copilot workspace discovery does not use that path. Migration
imports and backs up that obsolete file after the old merge mapping is removed;
prune deletes it only when its content still exactly matches generated legacy
output.

Official Pi deliberately has no built-in MCP support, so Agent Switch does not
create `.pi/mcp.json`. A Pi extension that defines its own MCP configuration can
still be wired through an explicit custom symlink mapping.

## Field Preservation

Agent Switch preserves unknown canonical frontmatter fields. During export, each
adapter writes the common fields it explicitly supports and moves values from
that tool's namespace into its native frontmatter or config shape. Unknown
canonical fields remain in the canonical file but are not emitted into formats
that do not understand them.

During import, native fields are stored under the matching tool namespace. The
imported tool owns its own namespace, while all unrelated top-level fields,
unknown custom fields, and other namespaces are retained. This lets a single
canonical file carry shared instructions plus per-tool metadata without forcing
every tool to understand every field.
