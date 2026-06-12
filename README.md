# AgentStitch

AgentStitch is a zero-runtime-dependency Rust CLI for synchronizing a canonical `.agents/`
directory with native coding-agent formats.

The binary name is `agentstitch`.

## Commands

```bash
agentstitch init
agentstitch setup
agentstitch setup --check
agentstitch sync
agentstitch sync --check
agentstitch sync --tool codex,copilot
agentstitch doctor
agentstitch mappings validate
agentstitch version
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

AgentStitch v1 reads `.agentstitch.yaml` by default. It does not read
`scripts/mappings.yaml`.

```yaml
version: 1
agents_dir: .agents
manifest: .agents/.sync-manifest.json
```

Run `agentstitch init` to create the default config, canonical directories, sample files,
and recommended `.gitignore` entries.

## Migration From Repo-Local Scripts

1. Add `.agentstitch.yaml` to the repo.
2. Keep canonical files under `.agents/`.
3. Run `agentstitch sync --check` in CI to detect drift.
4. Replace repo-local wrapper scripts with:

```bash
agentstitch setup
agentstitch sync
```

The Rust CLI v1 intentionally does not provide an automatic
`scripts/mappings.yaml` to `.agentstitch.yaml` migration command.

## Build

```bash
cargo build --release -p agentstitch-cli
```

The release binary is:

```text
target/release/agentstitch
```

## Test

```bash
cargo test
```
