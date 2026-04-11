# Claude Code Rev Gap Analysis And Fix Tracker

## Scope

这份文档用于跟踪 `yode` 与 `/Users/pyu/code/yode/claude-code-rev` 的关键差异，并把后续修复按优先级持续收口。

对比重点：

- 启动链路和 warmup
- 工具池装配与权限前置过滤
- shell 安全与权限解释
- provider 流式适配与共享协议层
- session / transcript / resume 恢复
- TUI 交互与渲染复杂度

状态标记：

- `[ ]` 未开始
- `[~]` 进行中
- `[x]` 已完成

## High-Level Gap Map

### 1. Startup / Bootstrap

- `claude-code-rev` 在 [src/main.tsx](/Users/pyu/code/yode/claude-code-rev/src/main.tsx) 中做了明显的启动 profiling、并行预热和条件加载。
- `yode` 当前启动链路已拆成 [main.rs](/Users/pyu/code/yode/src/main.rs) 和 [app_bootstrap.rs](/Users/pyu/code/yode/src/app_bootstrap.rs)，但缺少阶段级可观测性。

### 2. Tool System / Policy

- `claude-code-rev` 的工具装配集中在 [src/tools.ts](/Users/pyu/code/yode/claude-code-rev/src/tools.ts)，带有强 feature gating、权限上下文过滤、工具池装配逻辑。
- `yode` 的静态注册更清晰，但之前缺少更强的预过滤与工具池诊断，核心在 [builtin/mod.rs](/Users/pyu/code/yode/crates/yode-tools/src/builtin/mod.rs)。
- `claude-code-rev` 会在模型请求前按权限上下文裁剪工具池，并确保 ToolSearch 不会重新暴露 deny 掉的工具；这一点是 `yode` 过去最明显的工具调用差距。

### 3. Shell Security

- `claude-code-rev` 将 shell 安全分成 [bashPermissions.ts](/Users/pyu/code/yode/claude-code-rev/src/tools/BashTool/bashPermissions.ts) 和 [bashSecurity.ts](/Users/pyu/code/yode/claude-code-rev/src/tools/BashTool/bashSecurity.ts)。
- `yode` 当前已经有通用 guard，但 bash 专项策略还不够下沉，相关逻辑仍分散在 [guards.rs](/Users/pyu/code/yode/crates/yode-core/src/engine/tool_execution_runtime/single_call/guards.rs) 与 [permission/manager/mod.rs](/Users/pyu/code/yode/crates/yode-core/src/permission/manager/mod.rs)。

### 4. Session / Resume

- `claude-code-rev` 在 [sessionStorage.ts](/Users/pyu/code/yode/claude-code-rev/src/utils/sessionStorage.ts) 上投入很重，偏日志型恢复与缓存。
- `yode` 当前采用 `SQLite + transcript + live memory` 组合，结构更轻，但 resume/warmup 仍有优化空间，关键文件：
  - [db/mod.rs](/Users/pyu/code/yode/crates/yode-core/src/db/mod.rs)
  - [transcript/mod.rs](/Users/pyu/code/yode/crates/yode-core/src/transcript/mod.rs)
  - [transcripts/mod.rs](/Users/pyu/code/yode/crates/yode-tui/src/commands/info/memory/transcripts/mod.rs)

### 5. TUI / REPL

- `claude-code-rev` 的 REPL 和 PromptInput 很成熟，但复杂度极高：
  - [REPL.tsx](/Users/pyu/code/yode/claude-code-rev/src/screens/REPL.tsx)
  - [PromptInput.tsx](/Users/pyu/code/yode/claude-code-rev/src/components/PromptInput/PromptInput.tsx)
- `yode` 现在的 TUI 边界已经比以前清楚，但格式化与滚动层仍偏复杂：
  - [entry_formatting.rs](/Users/pyu/code/yode/crates/yode-tui/src/app/scrollback/entry_formatting.rs)
  - [markdown.rs](/Users/pyu/code/yode/crates/yode-tui/src/app/rendering/markdown.rs)

## Tracker

### P0

- `[x]` P0.1 启动链路增加阶段 profiler 和结构化日志
  - 目标文件：
    - [app_bootstrap.rs](/Users/pyu/code/yode/src/app_bootstrap.rs)
    - [main.rs](/Users/pyu/code/yode/src/main.rs)
  - 价值：
    - 先把 `cli_parse / config_load / tooling_setup / db_open / provider_bootstrap / session_bootstrap` 这些阶段量化出来，后续优化有依据。

- `[x]` P0.2 将 MCP/skills/provider warmup 从“顺序初始化”继续推进到“可控并行初始化”
  - 目标文件：
    - [app_bootstrap.rs](/Users/pyu/code/yode/src/app_bootstrap.rs)
    - [provider_bootstrap.rs](/Users/pyu/code/yode/src/provider_bootstrap.rs)
  - 当前完成：
    - skill discovery 改为独立 blocking task
    - MCP connect 改为并发连接
    - MCP tool discovery 改为并发发现、顺序注册
    - MCP tool register 与 connect 分阶段计时
    - database open 改为与 provider bootstrap 重叠执行
    - provider bootstrap 增加 provider 数量与耗时摘要
    - provider bootstrap 改为与 tooling setup 并行执行

- `[x]` P0.3 为启动 profile 增加用户可见诊断面
  - 候选出口：
    - `/status`
    - `/doctor`
    - 启动日志汇总
  - 当前完成：
    - `/status` 展示 startup profile
    - `/doctor` 展示 startup profile
    - 启动日志写入结构化 summary

### P1

- `[~]` P1.1 bash 权限与安全策略继续专门化
  - 把 bash 专项逻辑从通用运行时里继续下沉。
  - 当前完成：
    - 新增 `crates/yode-core/src/permission/bash.rs`
    - `PermissionManager::explain_with_content` 改为复用 bash 专项自动判定
    - `single_call` guard 改为复用 bash discovery redirect / destructive guard 文案
    - pattern rule explain 已指出具体命中 pattern 和命中的 command
    - permission artifact 已附带 bash classifier risk / rationale
    - shell denial 已按 command prefix 聚类并接到 `/permissions`、`/doctor`
    - safe readonly shell prefix inventory 已接到权限诊断面
    - repeated confirmation 的 bash prefix 已会产出规则化建议，避免用户长期手动重复确认

- `[ ]` P1.2 provider streaming 事件拼装进一步统一
  - 减少 Anthropic/OpenAI/Gemini 在 stream -> internal event 上的重复逻辑。

- `[~]` P1.3 resume / transcript 缓存增强
  - 增量索引、热路径缓存、恢复时 warmup 分层。
  - 当前完成：
    - resume transcript warmup 改为后台 blocking task，与 TUI startup 其它步骤重叠

### P2

- `[~]` P2.1 TUI 滚动/格式化层继续拆分
  - 当前完成：
    - `app/scrollback/entry_formatting.rs` 拆成目录模块
- `[~]` P2.2 工具池装配策略增加声明式过滤与诊断
  - 当前完成：
    - startup profile 增加 builtin / MCP server / MCP tool / skill / final tool count
    - `/status` 增加工具库存摘要
    - engine 新增 tool pool snapshot，区分 active/deferred、builtin/mcp、allow/confirm/deny
    - ChatRequest 构建前先按权限模式过滤 deny 工具，避免模型看到不可用工具
    - `tool_search` 改为尊重当前 tool pool，不再重新暴露被隐藏工具
    - `ToolRegistry` 改为支持运行时激活 deferred tool，`tool_search select:<tool>` 能把工具载入下一轮请求
    - tool activation 次数和最近一次激活已接到 `/tools`、`/status`、`/doctor`
    - startup 现在会在 tool-search 模式下把 MCP tools 注册到 deferred 池，而不是全部直接暴露
    - 当 tool-search 未启用时，`tool_search` 本身不再暴露给模型
    - tool turn artifact 已写入 tool pool snapshot，能追踪当时的 visible/hidden/deferred/activation 状态
    - tool-search 启用/关闭原因已接到 startup profile、`/status`、`/doctor`、`/tools`
    - `ToolRegistry` 现在会阻断并记录 duplicate registration，而不是静默覆盖
    - `/tools`、`/doctor` 会直接报告 command/tool 命名重叠
    - `/tools`、`/status`、`/doctor` 增加 model-visible / hidden 工具池诊断
- `[ ]` P2.3 tool/runtime/status 面板化而不是纯文本堆叠

## Notes

- 不建议把 `yode` 入口演进成 Claude Code 那种超大入口。
- 不建议把工具注册集中回单体巨型装配文件。
- `yode` 更适合保持 Rust 多 crate 的清晰边界，只补缺失的性能、权限和恢复能力。
