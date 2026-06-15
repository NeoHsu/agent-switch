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
canonical-first repositories with no native files to import. Teams can commit the
canonical files and usually ignore generated tool-native outputs.

## Default Integration Map

The default `.agent-switch.yaml` maps canonical files to tool-native paths as
follows. `link/copy` means `ags setup` creates managed links. On Windows,
directory link failures can fall back to junctions, and file symlink failures
can fall back to managed plain-file copies. Those file-copy fallbacks are
reconciled by `ags sync`. `generated` and `merged` paths are written by
`ags sync`.

| Canonical source | Claude | Codex | Copilot | OpenCode | Pi | Antigravity |
| --- | --- | --- | --- | --- | --- | --- |
| `AGENTS.md` | `CLAUDE.md` link/copy | — | — | — | — | — |
| `.agents/agents/*.md` | `.claude/agents` link/copy | `.codex/agents/*.toml` generated | `.github/agents/*.agent.md` generated | `.opencode/agents/*.md` generated | — | — |
| `.agents/commands/*.md` | `.claude/commands` link/copy | — | `.github/prompts/*.prompt.md` generated | `.opencode/commands` link/copy | — | `.agent/workflows` link/copy |
| `.agents/rules/**/*.md` | `.claude/rules` link/copy | — | `.github/instructions/**/*.instructions.md` generated recursively | — | — | `.agent/rules` link/copy |
| `.agents/skills/*` | `.claude/skills` link/copy | no managed output | no managed output | — | `.claude/skills` link/copy | `.agent/skills` link/copy |
| `.agents/mcp.json` | `.mcp.json` link/copy | `.codex/config.toml` merged marker block | `.copilot/mcp-config.json` converted/merged | `opencode.json` merged | `.pi/mcp.json` link/copy | — |

Tool-level view:

| Tool | Native paths managed by default | Canonical source | Notes |
| --- | --- | --- | --- |
| Claude | `.claude/agents`, `.claude/commands`, `.claude/rules`, `.claude/skills`, `CLAUDE.md`, `.mcp.json` | `.agents/agents`, `.agents/commands`, `.agents/rules`, `.agents/skills`, `AGENTS.md`, `.agents/mcp.json` | Direct managed-link integration; edits through real symlinks or Windows junctions affect canonical files immediately. |
| Codex | `.codex/agents/*.toml`, `.codex/config.toml` | `.agents/agents/*.md`, `.agents/mcp.json` | Agents are exported as TOML; MCP is rendered into an Agent Switch marker block. |
| Copilot | `.github/agents/*.agent.md`, `.github/prompts/*.prompt.md`, `.github/instructions/**/*.instructions.md`, `.copilot/mcp-config.json` | `.agents/agents`, `.agents/commands`, `.agents/rules`, `.agents/mcp.json` | Agents, prompts, and instructions are generated Markdown; MCP is converted to Copilot's config shape. |
| OpenCode | `.opencode/commands`, `.opencode/agents/*.md`, `opencode.json` | `.agents/commands`, `.agents/agents`, `.agents/mcp.json` | Commands are linked/copied; agents are generated with OpenCode metadata; MCP is merged into `opencode.json`. |
| Pi | `.claude/skills`, `.pi/mcp.json` | `.agents/skills`, `.agents/mcp.json` | Uses Claude-compatible skills plus a managed Pi MCP config link. |
| Antigravity | `.agent/rules`, `.agent/workflows`, `.agent/skills` | `.agents/rules`, `.agents/commands`, `.agents/skills` | Rules, workflows, and skills are exposed through managed links. |

Sync behavior by integration type:

| Integration type | Written by | Import behavior |
| --- | --- | --- |
| `link/copy` | `ags setup`; Windows file-copy fallbacks are reconciled by `ags sync` | `ags migrate` imports existing native files before replacing them with managed links, junctions, or file-copy fallbacks. Real symlink and junction edits directly update the canonical target. File-copy fallback edits can be copied back during `ags sync`. |
| `generated` | `ags sync` export stage | `ags migrate` imports existing generated files. Later `ags sync` can import tool-side generated edits back into `.agents/` unless `--export-only` is used. |
| `merged` | `ags sync` merge stage | `ags migrate` imports known native MCP shapes into `.agents/mcp.json`. Later sync merges canonical MCP config back to native configs. |

A dash (`—`) means Agent Switch has no default managed output for that tool and
canonical source type. A tool may still read `.agents/` directly if it supports
that behavior independently of Agent Switch.

## Agents

Canonical agents are Markdown files under `.agents/agents/`. Frontmatter fields
shared by multiple tools stay at the top level. Tool-specific fields live in a
tool namespace such as `copilot`, `opencode`, or `codex`.

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
| `codex-agent` | non-empty `name`, non-empty `description`, non-empty Markdown body |

Tool-specific generated edits can be imported back into the canonical file. If
both the canonical and generated file changed since the manifest was written,
the current import behavior chooses the tool-side content for that tool while
preserving other tool namespaces where possible.

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
| Antigravity | `.agent/rules` links or copies `.agents/rules` |
| Copilot | exports recursive `.github/instructions/*.instructions.md` |

## Skills

Canonical skills live under `.agents/skills/`, typically one directory per skill
with a `SKILL.md` file:

```text
.agents/skills/example-skill/SKILL.md
```

Default mappings expose skills through `.claude/skills`, `.agent/skills`, and
Pi's Claude-compatible skills path. Codex and GitHub Copilot can also discover
skills directly from `.agents/skills` in supported environments, so no generated
copy is needed for those tools.

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
| Copilot | converts to `.copilot/mcp-config.json` with Copilot `type` and `tools` fields |
| Pi | `.pi/mcp.json` links or copies `.agents/mcp.json` for Pi-compatible adapters |
| OpenCode | merges into `opencode.json` |
| Codex | merges into `.codex/config.toml` |

## Field Preservation

Agent Switch preserves unknown canonical frontmatter fields when possible. During
export, each adapter writes the fields required by the target tool and moves
namespaced values into that tool's native frontmatter or config shape. During
import, extra native fields are stored back under the matching tool namespace.

This lets a single canonical file carry shared instructions plus per-tool
metadata without forcing every tool to understand every field.
