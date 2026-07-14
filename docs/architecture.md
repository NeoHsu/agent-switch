# Agent Switch Architecture

本文件說明 Agent Switch 的整體架構、資料流、同步語意與主要擴充點，供維護者與貢獻者快速理解專案。

## 目標與核心模型

Agent Switch 是一個 Rust CLI，binary 名稱為 `ags`。它的目標是讓 repository 內維持一份 canonical agent 資料來源，並同步到不同 coding-agent 工具的原生檔案格式。

核心模型：

```text
.agents/                         canonical source of truth
├── agents/                      reusable subagents
├── commands/                    command / prompt definitions
├── rules/                       coding rules / instructions
├── skills/                      skill folders
└── mcp.json                     canonical MCP server config

.agent-switch.yaml               mapping config
.agents/.sync-manifest.json      generated/link state manifest
```

Agent Switch 只管理 config 宣告或 manifest 追蹤的輸出。遇到未管理的真實檔案或目錄時，會保守跳過，避免覆蓋使用者資料。

整體資料流：

```text
                         +--------------------+
                         | .agent-switch.yaml |
                         | mappings / tools   |
                         +---------+----------+
                                   |
                                   v
+-------------------+      +-------+-------+      +-----------------------------+
| .agents/          | ---> | SyncPlan      | ---> | generated tool-native files |
| canonical source  |      | jobs / links  |      | .github/ .opencode/ .codex |
+---------+---------+      +-------+-------+      +--------------+--------------+
          ^                        |                             |
          |                        v                             |
          |              +---------+----------+                  |
          |              | .sync-manifest.json|                  |
          |              | hashes / ownership |                  |
          |              +---------+----------+                  |
          |                        |                             |
          +------------------------+-----------------------------+
                   import tool-side edits / remove stale outputs
```

## Workspace Layout

```text
Cargo.toml
crates/
├── agent-switch-cli/
│   ├── build.rs                 build metadata: target, git SHA, build date
│   ├── src/main.rs              clap CLI, exit-code mapping, command dispatch
│   └── tests/cli.rs             CLI integration tests
└── agent-switch-core/
    ├── src/lib.rs               public modules, shared Error/ExitCode/CommandOutput
    ├── src/config.rs            config schema, defaults, path/tool validation
    ├── src/init.rs              `ags init`
    ├── src/migrate.rs           native tool files -> canonical migration
    ├── src/setup.rs             symlink/copy setup and prune
    ├── src/sync.rs              sync orchestration
    ├── src/sync/                sync plan, events, report, stages
    ├── src/formats/             markdown, copilot, opencode, codex adapters
    ├── src/mcp.rs               MCP merge adapters
    ├── src/fs.rs                filesystem helpers and atomic writes
    ├── src/manifest.rs          sync manifest load/save/hash
    ├── src/diagnostics.rs       doctor and mapping validation
    └── src/tool.rs              Tool/Format/MergeFormat enums and ownership rules
```

`agent-switch-cli` 只負責 CLI 介面與 process exit。主要邏輯集中在 `agent-switch-core`，方便測試與未來重用。

## Command Dispatch Flow

```text
+---------+      +------------+      +-------------------+
| ags CLI | ---> | clap parse | ---> | config::find_root |
+---------+      +------------+      +---------+---------+
                                             |
                                             v
                                  +----------+-----------+
                                  | parse global --tool  |
                                  | filter, if provided  |
                                  +----------+-----------+
                                             |
                                             v
                             +---------------+----------------+
                             | dispatch by subcommand          |
                             +---------------+----------------+
                                             |
       +----------+----------+----------+----------+----------+----------+
       |          |          |          |          |          |          |
       v          v          v          v          v          v
+------+---+  +---+------+  +---+----------------+  +------+-------+  +--+-------+  +--+----------+
| init     |  | migrate |  | setup              |  | sync         |  | doctor   |  | mappings    |
|          |  | cfg I/O |  | load config        |  | load config  |  | optional |  | validate    |
| init::run|  | run     |  | setup::run         |  | sync::run    |  | config   |  | load config |
+----------+  +----------+  +--------------------+  +--------------+  +----+-----+  +------+------+
                                                                         |               |
                                                                         v               v
                                                   +--------+------+  +-----+-----------+
                                                   | diagnostics   |  | diagnostics     |
                                                   | ::doctor_at   |  | ::validate      |
                                                   +---------------+  +-----------------+

version:
  ags -> version_output
```

Root discovery 順序：

1. explicit `--root`
2. 從 current directory 往上找：`.agent-switch.yaml`、`.agents` 或 `.git`
3. 找不到時使用 current directory

Config 預設讀取 `.agent-switch.yaml`，也可用 `--config` 指定。

## Config Schema and Validation

主要 config：

```yaml
version: 1
agents_dir: .agents
manifest: .agents/.sync-manifest.json
sync_mode: canonical-only

symlinks:
  CLAUDE.md: AGENTS.md
  .mcp.json: .agents/mcp.json

generate:
  codex-agents:
    from: .agents/agents
    to: .codex/agents
    format: codex-agent
    suffix: .toml
    recursive: false

merge:
  codex-config:
    to: .codex/config.toml
    format: codex
  antigravity-mcp-config:
    to: .agents/mcp_config.json
    format: antigravity
```

`config::validate_config` 會在 setup/sync 前執行，主要 invariants：

- `version` 必須為 `1`。
- 所有 repo path 必須是 repository-relative。
- 不允許 absolute path、`.`、`..` component。
- path 必須使用 forward slash，避免跨平台不一致。
- `generate.to` 不可重複。
- 未知或拼錯的 config 欄位會被拒絕。
- 同一 mapping 只能使用 `tool` 或 `tools` 其一。
- `tools` 不可為空，也不可重複。
- symlink path 與 target 不可相同。
- `sync_mode: canonical-only` 讓預設 `ags sync` 只從 canonical export 到 native adapters。

Tool filter 會套用在 `symlinks`、`generate`、`merge`：

- mapping 有 `tool/tools` 時使用顯式 ownership。
- 否則依 format 或內建 symlink path rule 推論 ownership。
- custom symlink 若沒有推論或顯式 ownership，預設所有工具都保留。

Pi 的 default mappings 只將 shared commands 暴露到 `.pi/prompts`；
`AGENTS.md` 與 `.agents/skills` 都由 Pi 直接探索。Pi-only extensions、
themes、project settings 維持 unmanaged。官方 Pi 沒有內建
subagent、rules directory 或 MCP，因此 default config 不會模擬這些不存在
的介面。

## Setup Architecture

`setup::run` 負責建立工具原生位置需要的 symlink、Windows directory
junction，或必要時的 file-copy fallback。
除了 config 內宣告的 `symlinks`，Claude 被選取時也會掃描 repository 內
nested `AGENTS.md`，並建立同目錄的 managed `CLAUDE.md` link/copy fallback。
掃描會跳過 `.agents/`、`.claude/`、`.github/`、`.git/` 等工具與管理目錄。

```text
setup::run
    |
    v
+-------------------+
| --prune enabled ? |
+----+----------+---+
     | yes      | no
     v          |
+-----------------------------+
| remove unselected managed   |
| links / copy fallbacks      |
+-------------+---------------+
              |
              |
              +----------+
                         |
     +-------------------+
     |
     v
+-----------------------------+
| iterate selected symlinks   |
+-------------+---------------+
              |
              v
+-----------------------------+
| is already correct symlink? |
+----------+------------------+
           |
    yes    | no
     |     |
     v     v
   [ok]  +---------------------------+
         | git symlink placeholder ? |
         +----------+----------------+
                    |
             yes    | no
              |     |
              v     v
+---------------------------+    +----------------------+
| repair link              |    | path already exists? |
| or report Drift in check |    +----------+-----------+
+---------------------------+               |
                                      yes    | no
                                       |     |
                                       v     v
              +--------------------------+  +-----------------------------+
              | skip unmanaged real file |  | create symlink or fallback |
              | / dir; report Drift      |  | file copy / junction      |
              +--------------------------+  +-----------------------------+
```

平台行為：

- Unix：建立 relative symlink。
- Windows：嘗試 symlink；directory 可 fallback 到 junction；file symlink
  失敗時 fallback 到 managed copy。
- 非 Unix/Windows：fallback 到 copy。

Prune 會移除未選工具的所有受管輸出：

- managed link 與 file-copy fallback；
- generated 輸出（內容必須符合 manifest hash，或可由 canonical source
  重新產生比對一致）；
- MCP merge 內容（`opencode.json` 的 `mcp` key、`.codex/config.toml` 的
  marker block、與 `.agents/mcp_config.json` 的 `mcpServers` key）；
- 清空後留下的工具目錄（只刪空目錄，不會動到使用者內容）。

Prune 只刪除可判定為 managed 的內容：未受管的真實檔案、hash 已變更的
managed copy、被修改過的 generated 輸出都會被跳過並回報，不會被刪除；
未完成的 setup/prune 會回傳 Drift。

`setup --check` 除了檢查 link/prune，也會以 check mode 執行後續
sync；只有搭配 `--no-sync` 才只檢查 link/copy fallback。

Setup 在 Windows 建立 file-copy fallback 時，會把 copy 的 hash 記錄到 manifest 的
`links` 區，後續 `SyncLinksStage` 據此調解內容。

## Sync Architecture

`sync::run` 是同步 orchestrator。它會先載入 manifest、建立 `SyncPlan`，再依固定 stage 順序執行。

```text
sync::run
    |
    v
+----------------+
| manifest::load |
+-------+--------+
        |
        v
+-----------------+
| SyncPlan::build |
| specs -> jobs   |
+-------+---------+
        |
        v
+------------------------------+
| stage gates                  |
|                              |
| sync_mode: full:             |
|   Import -> Export -> Stale  |
|   -> Links -> Merge          |
|                              |
| sync_mode: canonical-only:   |
|   Export -> Stale -> Links   |
|   -> Merge                   |
|                              |
| --import-only:               |
|   Import only                |
|                              |
| --export-only:               |
|   Export -> Stale -> Links   |
|   -> Merge                   |
+--------------+---------------+
               |
               v
+------------------+
| ImportStage      |  generated tool file changes -> canonical source
+------------------+
          |
          v
+------------------+
| ExportStage      |  canonical markdown -> tool-native files
+------------------+
          |
          v
+------------------+
| RemoveStaleStage |  delete tracked outputs whose sources are gone
+------------------+
          |
          v
+------------------+
| SyncLinksStage   |  reconcile managed copy fallbacks
+------------------+
          |
          v
+------------------+
| MergeStage       |  MCP canonical config -> tool config
+---------+--------+
          |
          v
+-------------------+
| --check enabled ? |
+----+----------+---+
     | yes      | no
     v          v
+-----------------------------+     +-----------------------------+
| report Drift or            |     | manifest::save              |
| SyncedNoChanges; no writes |     | report deterministic events |
+-----------------------------+     +-----------------------------+
```

Stage 行為：

| Stage | 目的 | 寫入對象 |
| --- | --- | --- |
| `ImportStage` | 匯入 tool-side generated edits | `.agents/...` |
| `ExportStage` | 產生工具原生格式 | tool output dirs |
| `RemoveStaleStage` | source 移除後刪除 tracked output | generated output |
| `SyncLinksStage` | 調解 managed copy；只警告未追蹤檔案 | link 或 target |
| `MergeStage` | merge canonical MCP config | tool config |

Sync options：

- `--check`：只偵測 drift，不寫入檔案；有 drift 時 exit code 為 `1`。
- `--import-only`：只跑 import stage。
- `--export-only`：跳過 import，只跑 export/remove stale/link copy/merge。
- `--json`：輸出固定 schema 的 machine-readable report。
- `--event-filter`：過濾 text/JSON event output。

## SyncPlan and Manifest

`SyncPlan::build` 會：

1. 依 tool filter 選出 generate specs。
2. 掃描 `from` 目錄下的 markdown source。
3. 套用 suffix 與 recursive 規則產生 jobs。
4. 檢查 output collision。
5. 建立所有 job destination set，供 stale removal 使用。

Manifest 檔案：`.agents/.sync-manifest.json`

```json
{
  "generated": {
    ".codex/agents/reviewer.toml": {
      "hash": "generated-output-sha256",
      "src": ".agents/agents/reviewer.md",
      "src_hash": "canonical-source-sha256"
    }
  },
  "links": {
    "CLAUDE.md": "copy-fallback-sha256"
  },
  "meta": {
    "version": 1,
    "tool": "agent-switch",
    "tool_version": "<ags-version>"
  }
}
```

Manifest 的用途：

- 判斷 generated file 是否被 tool-side 修改。
- 判斷 canonical source 是否也同時改動，並在 event 中標記 conflict。
- 移除 stale generated outputs。
- 追蹤 Windows file-copy fallback 的 managed copy hash。

## Format Adapters

Format adapter 位於 `crates/agent-switch-core/src/formats/`。

Canonical source 大多是 markdown + YAML frontmatter。各工具 adapter 負責在 canonical 與工具原生格式間轉換。

| Format | Tool | Export | Import |
| --- | --- | --- | --- |
| `copilot-agent` | Copilot | flatten frontmatter | restore `copilot:` namespace |
| `copilot-prompt` | Copilot | 同 agent | 同 agent |
| `copilot-instructions` | Copilot | `paths` -> `applyTo` | reverse mapping |
| `opencode-agent` | OpenCode | 加 `mode: subagent` | 移除 `mode`，補 `name` |
| `codex-agent` | Codex | markdown -> TOML | TOML -> markdown |

Export validation follows native hard requirements where the target tool
documents them:

- `copilot-agent` requires non-empty `name` and `description`.
- `codex-agent` requires non-empty `name`, `description`, and markdown body
  content, which becomes `developer_instructions`.

Import 時會保留其他工具 namespace，例如從 Copilot 匯入時保留
`opencode:`、`codex:`、`tools`、`model` 等 canonical metadata。

## MCP Merge

Canonical MCP config 來源是：

```text
.agents/mcp.json
```

Merge adapters：

- Claude 與 Copilot：直接探索 workspace `.mcp.json`，由 managed link/copy
  暴露 canonical `.agents/mcp.json`，不需要 schema conversion。
- OpenCode：將 `mcpServers` 轉成 `opencode.json` 的 `mcp` object，保留
  `enabled`、`cwd`、`timeout`、`oauth` 與其餘使用者設定，且不建立無關的
  settings。
- Antigravity：寫入原生 `.agents/mcp_config.json`；remote server 的 canonical
  `url` 會轉為 Antigravity 要求的 `serverUrl`，並保留其他 top-level 設定。
- Codex：將 MCP servers render 成 TOML，寫入 `.codex/config.toml` 的
  marker block，支援 stdio 與 Streamable HTTP server：

```toml
# >>> agent-switch:mcp >>>
[mcp_servers.example]
command = "npx"
# <<< agent-switch:mcp <<<
```

Codex merge 只替換 marker block，不覆蓋 marker 外的使用者設定。

## Filesystem Safety

`fs.rs` 集中處理檔案安全與跨平台細節：

- `read_text`：UTF-8 read，並容忍 leading UTF-8 BOM。
- `repo_path`：輸出路徑一律 forward slash。
- `write_if_changed`：內容相同不寫入；I/O error 不靜默吞掉。
- `atomic_write`：先寫 temp file、sync，再 rename replace。
- `relative_link`：產生 relative symlink target。
- `is_fake_symlink`：偵測 git checkout 後的 symlink placeholder text file。
- `remove_file_or_empty_dir`：跨平台刪 symlink/file/empty dir。

## Events, JSON Output, and Exit Codes

Sync event kinds：

```text
imported, generated, removed, copied, warning, merged, drift, synced_no_changes
```

JSON output 會 stable sort events，並包含 summary、options、exit 與 exit
code，方便 CI 或 script 使用。

Exit codes：

| ExitCode | Code | Meaning |
| --- | ---: | --- |
| `Ok` | 0 | 成功，無 drift |
| `Drift` | 1 | `--check`、`doctor` 或未完成的 setup/prune 偵測到 drift |
| `Config` | 2 | config 或 CLI 使用錯誤 |
| `Io` | 3 | I/O 或其他 runtime error |
| `Unsupported` | 4 | 不支援的 config version 或功能 |

CLI 的 `classify_error` 會將 core `Error` 對應到上述 exit code。
`doctor` 會檢查實際 config path、manifest、managed links 與 export-side
sync drift；不健康狀態回傳 Drift，config invalid 則回傳 Config 或
Unsupported。

## Build and Release

`agent-switch-cli/build.rs` 會注入：

- `TARGET`
- `GIT_SHA`
- `BUILD_DATE`

`BUILD_DATE` 支援：

1. explicit `BUILD_DATE`
2. reproducible build 用的 `SOURCE_DATE_EPOCH`
3. fallback 到目前 UTC epoch 轉出的 ISO-8601 字串

Release workflow 在任何 artifact build 前先執行 fmt、clippy、完整測試與
`cargo audit`，再針對 Linux/macOS/Windows target build archive；tag
pattern 為 `v*`。

## Testing and CI

主要測試類型：

- format round-trip：`crates/agent-switch-core/tests/formats.rs`
- setup/prune/config validation：`crates/agent-switch-core/tests/setup.rs`
- sync pipeline and JSON report：`crates/agent-switch-core/tests/sync.rs`
- CLI exit/output integration：`crates/agent-switch-cli/tests/cli.rs`
- README/docs command examples：CLI unit test 交給實際 Clap parser 驗證

CI 預期執行：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check --workspace --all-targets --locked      # MSRV toolchain
cargo audit --deny warnings
cargo test --workspace --locked                     # Linux, macOS, Windows
```

## Adding a New Tool or Format

新增工具/格式時，建議順序：

1. 在 `tool.rs` 新增 `Tool` variant，更新 `Tool::name` 與 ownership rules。
2. 若需要 generated adapter，在 `tool.rs` 新增 `Format` variant，實作 `Format::tool`。
3. 在 `formats/` 新增或更新 adapter，並接到 `formats/mod.rs` 的 `export/import` match。
4. 更新 `config.rs` 的 default mappings：`DEFAULT_SYMLINKS`、`DEFAULT_GENERATE` 或 `DEFAULT_MERGE`。
5. 若需要 config merge，更新 `MergeFormat` 與 `mcp.rs` 或新增 merge module。
6. 補 format round-trip test、sync integration test、tool filter test。
7. 更新 README 與本文件。

設計原則：

- Canonical `.agents/` 永遠是主要來源。
- Tool-specific metadata 應放在 tool namespace，例如 `copilot:`、`opencode:`、`codex:`。
- Generated output 必須 deterministic，避免 CI drift。
- `--check` 不可寫入檔案。
- 未管理的真實檔案/目錄不可被覆蓋或刪除。
