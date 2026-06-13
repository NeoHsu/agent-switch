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

`ags init` creates starter directories and sample files. Teams can commit the
canonical files and usually ignore generated tool-native outputs.

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
