# Yode 项目优化清单

> 生成日期：2026-06-17（含桌面应用迁移分析）
> 分析范围：架构设计、UI/UX、冗余代码、技术债/健壮性四个维度，覆盖 6 个 crate + Tauri 桌面端（前端 + src-tauri）。

## 概览

代码规模：
- Rust：约 **13.8 万行**（不含 vendored ratatui 的 4.6 万行），6 个 crate
- 桌面前端：约 **1.86 万行** TS/TSX/CSS（React 18 + Vite + Tailwind v4 + zustand）
- 桌面后端 src-tauri：约 **5250 行** Rust（runtime.rs 4249 / lib.rs 596 / protocol.rs 397）

**当前状态：双轨期。** TUI（yode-tui，68k 行）仍是 CLI 唯一入口与开发主干；桌面应用（apps/yode-desktop）已成为发布产物（release.yml 6/17 切到只发桌面）。README 的"No real AgentEngine bridge yet"已**过时**——桥接真实且完整打通。

整体工程质量较高：
- 零 `unsafe`、零真实 `TODO`/`FIXME`、死代码极少
- 权限/hook 子系统设计良好，可作为重构范本
- 无循环依赖，依赖分层总体合理
- 常量管理有编译期断言（亮点）
- 桌面前端零 `ts-ignore`/`eslint-disable`，终端 PTY 集成真实且质量高

问题集中在七个方面：**上帝对象、i18n 半成品、错误处理无类型化、信息密度失控、纯重复样板、桌面状态管理混乱、双轨期共享层缺失**。

问题集中在五个方面：**上帝对象、i18n 半成品、错误处理无类型化、信息密度失控、纯重复样板**。

优先级图例：🔴 P0（结构性/高风险）｜🟠 P1（显著影响）｜🟡 P2（一致性/维护性）｜⚪ P3（打磨）

---

## 一、架构设计

### 🔴 P0 — 结构性问题

**A1. `AgentEngine` 是上帝对象（146 个字段）** — `crates/yode-core/src/engine.rs:58-387`

扁平追踪 12+ 个不相关关注点：压缩状态（~30 字段）、prompt 缓存（hash/text × system/restore/tool/message ≈ 16 字段）、恢复状态机、工具遥测、预算通知…任何修改都要借用整个结构体。

→ 拆分为 `CompactionState`、`PromptCacheState`、`RecoveryState`、`ToolTelemetry` 等子结构体，构造函数传 `&mut SubState` 而非 `&mut self`。

**A2. `App`（TUI）是上帝对象（51 个字段）** — `crates/yode-tui/src/app/state/mod.rs:30`

输入编辑、历史、两个补全引擎、思考状态、7 个 streaming_markdown 缓冲、确认通道、计时、引擎句柄…混杂一处；状态流转靠散落的 `is_processing`/`is_thinking`/`should_quit` 标志隐式驱动，而非显式状态机。

→ 抽 `InputState`/`CompletionState`/`StreamingRenderState`，状态用 enum 显式建模。

**A3. `yode-core` 巨型 crate（28k 行，18 项职责）** — `lib.rs` 暴露 config/context/db/engine/hooks/permission/updater/transcript/skills/plugins…

SQLite、权限引擎、hook、自动更新器、转录全捆在一个 crate。`yode-desktop` 已需要其中一部分但被迫依赖整个引擎。

→ 至少拆出 `yode-db`、`yode-permission`、`yode-hooks`、`yode-updater`。

**A4. `vendor/ratatui` 全量 vendored 但几乎零定制** — 4.6 万行第三方代码直接提交

仅 2 个 commit 动过它，且只改 `Cargo.toml`+`lib.rs` 共几行；grep 未见真实 fork 补丁（命中全是误报）。所谓的"动态视口高度/内联粘贴 pill"实际在 yode-tui 侧实现。

→ 改回 `ratatui = "0.29"`（crates.io），用 patch file 承载那两处小改动；移除 4.6 万行负担、恢复上游更新能力。

### 🟠 P1

**A5. 两个并行轮次循环实现，结构重复且易漂移** — `streaming_turn_runtime/mod.rs:44` vs `nonstream_turn_runtime.rs:10`

两者都走 `rebuild_system_prompt → append_turn_setup → record_turn_user_input → reset_turn_runtime_state → loop{...}`，错误恢复分支（`should_reactive_strip_media_error` 等）重复。

→ 统一为共享 turn orchestrator，仅在 LLM 调用策略（流式/批量）上分叉。

**A6. `yode-core` 依赖 `crossterm`（层级倒置）** — `yode-core/Cargo.toml` + `setup.rs:3`

核心引擎库触碰终端 raw mode + 按键读取做首次运行向导，污染依赖图，使 core 无法脱离终端使用。

→ 把交互式 setup 移到 yode-tui/二进制层。

**A7. "代理团队"概念分裂在三个 crate** — `yode-agent`（纯状态）+ `yode-tools/team_runtime`（持久化）+ `yode-core/subagent_runner`（执行，1214 行）

`SubAgentRunner` trait 定义在 yode-tools 里，执行在 yode-core，命名 `yode-agent` 却不含运行逻辑——边界任意、命名倒置。

→ 持久化归并到 `yode-agent`，考虑改名 `yode-team`。

**A8. engine 过度拆分为 ~22 个 `*_runtime` 文件，粒度极不一致** — `engine.rs:1-20`

`_runtime` 后缀失去语义；微型文件（`tool_pool_runtime` 91 行）与巨石（`compaction_runtime/mod.rs` 1388 行、`subagent_runner.rs` 1214 行）并存。

→ 重组为按职责命名（非按"runtime"后缀），拆分 compaction_runtime 巨石。

### 🟡 P2

- **A9. 流接收循环两个 `loop{}` 主体几乎重复**（cancel_token Some/None 分支）— `stream_loop.rs:33-110`
- **A10. 命令系统过度碎片化**：120 个文件却混入 5 个 2k+ 行巨石（`remote_control_workspace.rs` 2679、`inspect.rs` 2130…）— 碎片化只重分配复杂度
- **A11. `engine/tests/` 约 3k 行集成测试混在 src/**（`runtime.rs` 1217、`compaction.rs` 941）— 移到 crate 级 `tests/` 解耦并加速增量构建
- **A12. 工具结果路径横跨 yode-tools（注册）与 yode-core（运行时/遥测/结果）无清晰接口** — engine 直接深入 tools 内部

---

## 二、UI/UX

### 🔴 P0 — 严重影响可用性

**U1. i18n 系统形同虚设，中英文混排** — `i18n.rs` 全仓仅 3 处调用；`en.toml` 的 `[tools]` 翻译从未被读取

29 个文件、604 行硬编码中文：`display_text.rs:10`（工具名"命令/读取/写入"）、`state/types.rs:208` SPINNER_VERBS、`turn_status.rs:96`"已完成"、`chat_entries/tools.rs`"（Ctrl+O 查看）"。英文用户默认得到破碎的混排界面（"Ask anything…" 英 + "命令" 中 + "done" 英 + "已完成" 中）。

→ 要么补全 i18n 接线，要么明确只做中文并移除 i18n 空壳。

**U2. 确认模式下聊天区完全留白** — `ui/mod.rs:91-104`

工具审批时整个对话区不渲染，只画状态行 + 14 行确认面板。用户失去上下文，无法判断该工具调用是否合理。

→ 确认面板悬浮底部，聊天保持可见（参考 Claude Code）。

**U3. 主题系统名存实亡** — `theme.rs` 接受 `light`/`dark` 并提示重启，但 `palette.rs` 全硬编码深色 `Indexed(23x)`，**无任何代码读取 theme 字段切换调色板**。light 主题无效。

→ 要么实现 light 调色板，要么移除该命令避免误导。

### 🟠 P1

**U4. 状态信息行严重过载** — `status_bar.rs:38-205`

单行塞入 15+ 字段（权限模式/回合状态/耗时/模型/token↑↓/工具数/成本/context/compaction/memory/prompt-cache/runtime-cost/队列/后台/诊断/预算/快捷键提示）。wide 模式认知负荷极高，任何项变长即截断。

→ 拆为两行或按重要性分级（主行：模式+模型+token+成本；副行：其余 badge）。

**U5. 快捷键参考与实现脱节** — `commands/utility/keys.rs:56-92` 用 `concat!` 硬编码列表，非从 `key_dispatch.rs` 生成

缺失：`Ctrl+T`（模型选择 wizard）、`Ctrl+O`（最近工具 inspector，聊天里却处处提示）、`Ctrl+V`（粘贴，仅 macOS 硬编码）、`Ctrl+E`（确认态解释）。高频功能零文档。

→ 从 dispatch 表自动生成 `/keys`，消除漂移。

**U6. Header 永久占用 ~8 行** — `chat_layout.rs:400-514`

带边框 ASCII logo 盒子在 24 行终端占 1/3 垂直空间，`should_show_logo` 只关 logo 不关盒子。

→ 默认收起，小终端自动隐藏。

**U7. 双击 Ctrl+C 首次清空输入无撤销** — `key_dispatch.rs:416-424`

输入非空时首次 Ctrl+C 直接清空而非提示退出，误操作损失大且无 undo。

→ 改为提示"再按一次退出"，或保留输入仅退出。

### 🟡 P2 — 一致性与可维护性

- **U8. 命令重叠**：`/status` vs `/brief` vs `/diagnostics` 职责模糊；`/cost` 与状态栏重复；6 个 Dev "prefill" 命令（review/reviews/pipeline/ship/coordinate/bug）可合并为 `/template <name>`
- **U9. 死代码命令**：`ProvidersCommand`（`model/providers.rs`）定义但 `register_all` 未注册
- **U10. detail_inspector 样板膨胀**（2026 行）：大量近重复的 `build_*`/`*_action`/`*_badges` 函数
- **U11. 低对比度色**：`palette.rs` 的 `HINT_COLOR(239)`/`GHOST_COLOR(242)` 接近背景 `SURFACE_BG(236)`，占位文本难辨
- **U12. 流式解析脆弱**：`engine_events/streaming.rs:27` 用子串匹配 `[tool_use`/`name=bash` 探测标签边界，内容含这些子串即误判
- **U13. inspect 命令与 Ctrl+O 路径重叠** — `inspect.rs`(2130) 与 `detail_inspector.rs::open_latest_tool_inspector` 职责重叠

### ⚪ P3 — 打磨

- Markdown 标题/代码色硬编码（`renderer.rs:54-63`、`structured_diff.rs:113-130`）不随 palette 联动
- `/help` 末尾引用别名 `/keybindings` 而非主名 `/keys`
- 命名不一致：单词 vs 连字符（`remote-control`）vs 复数混用；`/status`/`/cost`/`/files` 高频命令缺别名
- Shift+Tab/Esc 多重重载虽上下文合理，但缺当前焦点视觉指示，用户难预判行为

> 渲染引擎本身（markdown、结构化 diff、表格自适应、unicode 换行、inspector 多面板）工程质量较高——问题不在渲染能力，而在信息密度与 i18n。

---

## 三、冗余代码

纯重复项合计 **~510 行可直接消除**，均为低风险机械重构。

| # | 问题 | 位置 | 可消除 |
|---|------|------|--------|
| R1 | **48 个 `latest_*_artifact` 样板包装** | `artifact_nav.rs`（1194 行，62 函数中 48 是薄包装） | ~200 行 |
| R2 | **`McpServerConfig` 系列 5 结构体两 crate 逐字重复** | `yode-core/config.rs:261` + `yode-mcp/config.rs:1`（yode-mcp 未复用 core，自维护副本） | ~80 行 |
| R3 | **git_* 测试/命令样板 + 工作区逃逸两套实现** | `git_{diff,log,status,commit}/mod.rs` ×4（`init_git_repo`/`ctx_with_dir` 重复 6 处；`String::from_utf8_lossy` 12 处） | ~120 行 |
| R4 | **`truncate_preview`/折叠函数散落 4-5 处逐字一致** | `checkpoint_workspace.rs:1459` + `remote_control_workspace.rs:2210` 等 | ~60 行 |
| R5 | **`merge_metadata` + diff metadata 三处重复** | `edit_file`/`multi_edit`/`write_file` 各一份 | ~60 行 |
| R6 | **`ApplyPatchTool` 独立实现文件补丁**（与 edit_file/write_file 平行未复用） | `codex_compat.rs:245` `apply_codex_patch` | ~50 行 |

### 🟠 逻辑重复（需谨慎合并）

- **R7. `subagent_runner.rs` 前台/后台双路径**（1214 行，43 clone）：两套并行执行路径重复 register_tools / AgentEngine 构造 / turn_prompt / sync_team_runtime_update / hook 发射，错误处理逐字出现两次 → 合并为带 `background: bool` 的统一实现，~80 行
- **R8. `multi_edit` 重复 `edit_file` 替换循环**，却缺少 `locate_edit_target`/`relaxed_line_match` 模糊匹配；预读检查还更弱（不 canonicalize）→ 委托 edit_file 内部逻辑

### 🟡 性能优化（非消除）

- **R9. `runtime_support.rs` 58 处 clone 快照**：`Option<String>`/`Option<PathBuf>` 改 `Option<Arc<str>>`/`Option<Arc<Path>>` 可把堆分配变原子计数
- **R10. 工作区逃逸校验两套实现**：`file_diff::resolve_workspace_file`（canonicalize）vs `git_diff::validate_path_filter`（仅查 `..`）→ 统一安全语义

> 死代码极少：仅 2 处 `#[allow(dead_code)]`（`inspector.rs:161,174`）+ 2 行注释残留（`edit_file/mod.rs:157`）。`team_runtime`(1938)、`codex_compat`(1644) 经核查是真实使用的功能层，非冗余。

---

## 四、技术债 / 健壮性

### 🔴 P0 — 高风险

**T1. `ProviderRegistry` 用 `std::sync::RwLock` 且 unwrap 锁 — 中毒级联** — `yode-llm/src/registry.rs:2,309,313`

`write().unwrap()` 在锁中毒后永久 panic。ProviderRegistry 是 LLM 调用核心路径，任一线程持锁 panic → 整个注册表永久不可用，所有后续 LLM 调用 panic。

→ 换 `parking_lot::RwLock`（无中毒）或处理 `PoisonError`。

**T2. 多 Agent 协调器静默丢弃 9 处状态持久化错误** — `yode-tools/src/builtin/coordinator/mod.rs:327,398-399,414,417,446-461`

`persist_agent_team_snapshot`/`hydrate`/`update_member` 全部 `let _ =`。磁盘失败时内存与磁盘静默分歧，团队状态损坏无日志无告警。

→ 至少 `tracing::warn!` 记录，关键路径返回错误。

**T3. `team_runtime` 等在 async 中阻塞 `std::fs::`** — `team_runtime/mod.rs`（23 处）、`mcp_resources`（15）、`updater`（11）；全项目 279 处

阻塞 I/O 跑在 tokio worker 线程上，阻塞整个 runtime。高风险点是大文件读写（transcript、artifact）。

→ 改 `tokio::fs::` 或 `spawn_blocking` 包装。

### 🟠 P1

**T4. 错误处理：全局 anyhow 泛滥，几乎无类型化错误** — 463 处 anyhow；仅 2 个 thiserror 枚举（`EngineError` 3 变体 + updater）

工具执行/LLM 调用/权限检查无法 `match` 失败模式，调用方只能拿不透明 `anyhow::Error`。另有 498 处 `let _ =`、402 处 `.ok()`（多数合理，但 T2 是危险例外）。

→ 为 LLM/工具/MCP 定义领域 `thiserror` 枚举；审计 498 处 `let _ =`。

**T5. LLM 响应转换中 `panic!`/`unreachable!`** — `yode-llm/src/providers/anthropic/request_conversion.rs:32,382,400,487`

处理外部 API 返回内容块时 panic。外部输入不应触发 panic——API 格式变化直接崩。

→ 返回 `Result`，优雅降级。

**T6. `registry.rs:488` catch_unwind 后重新 panic** — 工具执行 panic 被捕获后又 `panic!("poison registry tools lock")`，使防御失效。

→ 返回错误而非 panic。

### 🟡 P2

- **T7. 依赖版本不一致**：toml 3 版本（0.8/0.9/1.1，多为传递依赖）；winnow/hashbrown/nix 各 4 版本；`yode-core` 的 `tempfile` 同时在 deps 和 dev-deps（冗余）；`yode-desktop` 的 serde 直接指定版本而非 workspace 引用
- **T8. 测试覆盖**：yode-llm 仅 17%（provider 请求转换/流式解析偏低）；yode-mcp/yode-agent 无独立测试文件
- **T9. 文档严重不足**：6 个 crate 的 `lib.rs` **均无** `//!` crate 级文档；428 个 pub fn 中仅 106（~25%）有 `///` 文档
- **T10. 魔法数字散落**：`constants.rs` 组织良好（含编译期断言，是亮点），但 `shell_runtime.rs`、`web_fetch:69`、`web_search:125`、`task_output:133` 各自硬编码超时，可集中到配置

> **重要纠正**：表面 unwrap 热点（team_runtime 66、inspect 57、remote_control_workspace 55、mcp_resources 42）**全部位于内联 `#[cfg(test)]` 模块**，生产代码真实 unwrap 仅 ~32 处（多为 `Regex::new().unwrap()` 静态正则，风险极低）。零真实 unsafe、零真实 TODO——这两项纪律优秀。

---

## 五、建议执行顺序

1. **快速止血**（低风险高收益）：R1–R6 纯重复消除（~510 行）、U9 死命令、A4 去 vendor ratatui
2. **健壮性 P0**：T1 锁中毒、T2 协调器静默失败、T3 阻塞 I/O
3. **体验 P0**：U1 i18n 定调（补全 or 移除空壳）、U2 确认态留白、U3 主题系统
4. **结构性重构**：A1/A2 上帝对象拆分、A3 拆 crate、A5 统一轮次循环
5. **持续打磨**：U4 状态行分级、U5 快捷键自动生成、T4 类型化错误、T9 文档

> 权限和 hook 子系统是设计良好的范本，重构 engine/app 时可参照其模块化边界。

---

## 六、桌面应用分析（apps/yode-desktop）

> 技术栈：Tauri v2 + React 18 + TypeScript + Vite + Tailwind v4 + zustand + xterm + marked + highlight.js
> 前端 ~1.86 万行（41 tsx + 10 ts + 1 css），src-tauri ~5250 行

### 6.1 桥接现状（README 已过时，桥接真实打通）

**真实 AgentEngine 接入，非 mock。** 后端 `runtime.rs:1433-1486` 在每个 turn spawn 真正的 `yode_core::engine::AgentEngine`，调用 `run_turn_streaming_with_images`，把 27 个 `EngineEvent` 变体逐一映射成 `DesktopEvent` JSON 经 Tauri `app.emit("desktop-event", ...)` 推送；前端 `App.tsx:875-945` 用 `listen<DesktopEvent>` 消费并由 `timelineUtils.ts:945` 的 `applyDesktopEventToTimelineItems` reduce 成 `TimelineItem`。与 TUI 走的是**同一个 `AgentEngine`**。

- 后端注册 **55 个 Tauri command**（`lib.rs:536-593`），覆盖 bootstrap/sessions/turn/permission/ask_user/cancel/mcp/hooks/git/worktree/terminal/providers/config 等
- 前端 **42 处 `invoke`**
- `src/lib/mock.ts` 仅作非 Tauri 环境（纯浏览器 `pnpm dev` 预览）兜底，非生产链路；但它同时承载类型定义，命名误导，应拆 `types.ts` + `previewData.ts`

**迁移完成度**：核心对话闭环（会话/流式/工具/权限/ask_user/cancel）+ 基础设施（MCP/Skills/Hooks/Terminal/Providers/Worktree/Personalization/Configuration）已真实可用。**缺失集中在用户交互层**：52 个 slash 命令、交互式 Inspector、命令补全、transcript/artifact 导航、checkpoint/rewind、plugins/coordinator/team/remote-control。

### 6.2 桌面前端问题

#### 🔴 P0 — 架构性

**F1. `App.tsx` 上帝组件（1568 行 / ~30 个 useState）** — `src/App.tsx:317-1568`
所有业务、IPC、事件监听、拖拽、主题、消息发送/取消/队列/归档集中一处。
→ 引入真正的 store 拆分 session/timeline/settings/appearance 切片。

**F2. 状态管理三套真相源且无 store** — `useState` + `localStorage`（~50 个 `yode-*` key）+ `window.dispatchEvent` 事件总线（~40 种字符串事件）
`zustand ^4.5.5` 在依赖里却**全前端零使用**。同一状态多处读取导致不同步：语言/主题/设置在 App、Sidebar、Topbar、SettingsShell 各自 `localStorage.getItem`。`loadGeneralSettingsPayload()` 在 App 与 SettingsShell 重复定义。
→ 用 zustand + persist 中间件统一，移除手写事件总线。

**F3. IPC 类型手动双写无校验** — `protocol.rs`（Rust serde）与 `mock.ts`（TS）手工对应
后端 `DesktopEvent.payload` 是动态 `serde_json::Value`，前端 `TimelineItem` 强联合类型是单方面假设，`applyDesktopEventToTimelineItems` 用 ~25 处 `as any` 拼装。
→ 生成 TS 类型（ts-rs / specta）或至少共享 schema；给 `DesktopEvent` 按 kind 建类型映射。

#### 🟠 P1 — 类型与质量

**F4. `as any` / `: any` 共 126 处** — 集中在 `App.tsx:886-928`（事件）和 `timelineUtils.ts:110-521`（联合类型未 narrow）
→ 用 type guard 替代 `as any`。

**F5. 测试覆盖极薄**：仅 4 个测试文件（timelineUtils/ToolUtils/MarkdownContent/LiveStatusRow），核心 App/Composer/Terminal/Settings **0 测试**。

**F6. i18n 缺失，重演 TUI 问题**：无 i18n 库，~130 处 `appLang === "zh"` 三元分布在 14 个文件。系统通知/CodeBlock 文案硬编码中文未国际化（`App.tsx:904,913,918`、`CodeBlock.tsx:77,111`）。

#### 🟡 P2 — 功能性占位

**F7. RunInspector 是占位 stub**（48 行，全硬编码标签）— `src/components/RunInspector.tsx`。名为"运行详情"却无真实数据，对比 TUI 的 `detail_inspector.rs`(2026行)+`ui/inspector.rs`(1878行) 差距巨大。

**F8. 完全无 slash 命令系统** — `Composer.tsx` 任何输入都作为用户消息直送引擎，不解析 `/` 命令。这是从 TUI 迁移过来用户体感最大的落差。

**F9. Sidebar 导航死控件**：搜索/技能/插件/自动化 4 个 NavButton 无 onClick（`Sidebar.tsx:500-503,623-630`）。

**F10. keyboardShortcuts 定义 46 条但大量无实现**：`sidechat/newwin/find/addressbar/open_browser_tab/start_dictation/toggle_voice` 等在 App switch（`App.tsx:1400-1461`）无 case。

**F11. PersonalizationSettings 半接通**：从后端读（`personalization_state_get`），但修改只写 localStorage 无 apply（`PersonalizationSettings.tsx:14-26`）。

**F12. Topbar 分支名假数据**：固定 `<span>main</span>`（`Topbar.tsx:75`）。

#### ⚪ P3 — 维护性

- **F13. 545 处内联 `style={{}}`**，设置面板尤甚（McpSettings 63/Hooks 61/Environments 60），与 6034 行 app.css 并存，样式难追踪
- **F14. Tailwind v4 形同虚设**：仅 `@import`，全项目无 utility class 用法，依赖与实际不符
- **F15. SettingsShell 1059 行**含整个"常规"面板内联（200-960），应拆 `GeneralSettings`
- **F16. ChatWorkspace 三重复 running 检测函数**（`hasLiveProcessItem`/`hasRunningVisibleItem`/`isLiveTailStatusItem`，`ChatWorkspace.tsx:27-57`）逻辑近乎相同
- **F17. 流式重渲染无 memoization**：0 个 `React.memo`、无虚拟化，长对话每 token 全量重渲
- **F18. `preprocessMarkdown` 130 行正则补偿**（`MarkdownContent.tsx:105-233`）：反映后端流式 markdown 残缺，应在后端保证完整性
- **F19. 可交互折叠 div 缺键盘/a11y**：`ProcessNoteNode:33`/`ReasoningNode:25`/`InlineToolGroup:18` 的 `onClick` div 无 role/tabIndex
- **F20. `Topbar.providerOptions` useMemo 依赖 `[]`**（`Topbar.tsx:66`）：provider 列表变更不更新

> 桌面前端亮点：终端 PTY 集成真实且质量高（`TerminalDrawer.tsx`）；CodeBlock 截断检测与文件 chip 体验细腻；权限/提问卡片键盘导航完整；CSS token 体系用 oklch 较现代；0 处 ts-ignore/eslint-disable。

### 6.3 桌面后端问题（src-tauri）

**B1. `runtime.rs` 单文件 4249 行** — `turn_send_message` 630 行（1222-1852）+ 50+ 设置 getter/setter + worktree/terminal/import 等近 2000 行自由函数全混在一处。
→ 按域拆分 settings/worktree/terminal/import/turn 子模块。

**B2. 桌面 Rust 端零 CI 门禁** — `ci.yml` 的 `YODE_CLI_PACKAGES` 不含 `-p yode-desktop`，5247 行无 clippy/test 保护。
→ 立即把 `-p yode-desktop` 加入 CI。

**B3. EngineEvent 呈现映射重复** — `runtime.rs:1513-1720` 与 TUI `engine_events/streaming.rs`(307行) 是同一份 `EngineEvent` 枚举翻译到两个目标，新增事件变体必须改两处。
→ 抽共享 `RuntimeEvent` 层（见第七章）。

**B4. Updater apply 未接桌面** — `runtime.rs` 仅 emit `update_available/downloading/downloaded` 事件，apply + 重启逻辑只在 `src/main.rs:132-160`（CLI），桌面无法应用更新。

**B5. 多处 `setTimeout` 硬编码等待动画**（`TerminalDrawer.tsx:282-290` 230/240ms）等，脆弱。

---

## 七、迁移差异与后续策略

### 7.1 双轨现状

| 维度 | TUI（yode-tui） | 桌面（yode-desktop） |
|---|---|---|
| 角色 | CLI 唯一入口 + 开发主干 | 发布产物（release.yml 6/17 切到只发桌面） |
| 规模 | 68k 行 / 210 文件 | 前端 18.6k + src-tauri 5.2k |
| CI | 在 `YODE_CLI_PACKAGES` 内，有 fmt/clippy/test 门禁 + parity 基线 | **Rust 端无 CI 门禁** |
| 引擎 | 共享 `yode-core::AgentEngine` | 共享 `yode-core::AgentEngine` |
| 引用关系 | 仅 `src/main.rs` 引用 yode-tui | 桌面完全不引用 yode-tui（合理） |

**关键事实**：TUI 仍活跃（近 60 天 242 次提交，比桌面 116 次更多），非死代码。当前处于"TUI 退为 CLI/开发、桌面成主流 GUI"的转折点。

### 7.2 功能对齐差异（TUI 52 命令 vs 桌面）

| 类别 | 桌面状态 |
|---|---|
| **Session(10)** | sessions CRUD 有命令；compact/checkpoint/rewind/plan/rename/init/clear **无手动触发入口**（仅被动显示引擎事件） |
| **Model(3)** | provider/model 切换已迁移；**effort 缺失** |
| **Tools(6)** | MCP 完整；permissions 有；skills 后端已注册但**无前端 UI**；tools/plugin/workflows **缺失** |
| **Info(16)** | 仅 config/diagnostics/hooks/update 有对应；help/brief/status/cost/version/context/files/inspect/memory/teams/tasks/doctor **全缺** |
| **Dev(9)** | bench/diff/bug/coordinate/remote-control/review/reviews/pipeline/ship **全缺**（底层 builtin 仍可被模型调用，无用户命令面） |
| **Utility(8)** | copy 由系统剪贴板替代；keys/output-style/history/jump/theme/export **全缺** |

非 slash 功能：Inspector 缺（RunInspector 是占位）；Permission/Hooks/MCP 已迁移；Skills 后端有前端无；Plugins/Coordinator/Team/Remote-control/Workflow 缺；Compaction 仅被动显示；Updater 半迁移（事件有，apply 无）；Terminal/PTY、Browser、Computer-use、Tray 菜单栏驻留是桌面新增。

### 7.3 重复逻辑（本应共享却分散三处）

核心引擎循环**没有**被复制——两端都直接复用 `yode-core::AgentEngine`。真正重复的是"适配器/引导层"：

| 重复逻辑 | TUI 侧 | 桌面侧 |
|---|---|---|
| EngineEvent → UI 事件翻译 | `engine_events/streaming.rs`(307行) | `runtime.rs:1513-1720` |
| 权限审批状态机 | `ui/tool_confirm.rs` + app | `runtime.rs:1559-1583` + `permission_respond` |
| 会话恢复/解码 | `src/app_bootstrap/session_restore.rs`(14.4k) | `runtime.rs:2606` `stored_message_to_message` |
| Provider 引导 | `src/provider_bootstrap.rs`(14.2k) | `runtime.rs:2284` `bootstrap_providers` |
| Tooling 装配 | `src/app_bootstrap/tooling.rs`(11.2k) | `runtime.rs:2357` `setup_desktop_tooling` |
| 权限配置 | `src/app_bootstrap/mod.rs` | `runtime.rs:2421` `configure_desktop_permissions` |

**架构根因**：`src/app_bootstrap/` 属于根 `yode` 二进制 crate（非 lib），桌面端无法 `use` 它，只能复制。

### 7.4 共享层污染需治理

`yode-core/src/setup.rs:3` `use crossterm::{... enable_raw_mode ...}` —— 被 desktop 依赖的 core 拉进了终端 raw-mode 依赖，仅为 `run_setup_interactive()`。违反"core 不绑 UI"原则，应把交互式 setup 移出 core。

### 7.5 数据兼容性（已兼容，无需动）

两端共用 `~/.yode/`（`sessions.db`/`config.toml`/`session_memory`/`edit-diffs`/`memory`），路径解析均在 `yode-core`。**TUI 创建的会话能被桌面列出与打开**。桌面额外有 `~/.yode/desktop-settings.json`（私有 UI 偏好）和 `import_ai_sessions`（从外部 JSONL 导入）。

### 7.6 后续迁移路线（分阶段）

**阶段 0 — 止血与门禁（立即）**
- 把 `-p yode-desktop` 加入 `ci.yml` 的 clippy/check/test（B2）
- 冻结 TUI 新功能，明确进入维护期，避免两端差距继续扩大

**阶段 1 — 抽共享 runtime 层（1-2 周）**
- 新建 `crates/yode-runtime`，下沉第 7.3 节 6 处重复：
  - `EngineEvent → RuntimeEvent` 标准化翻译（消除双份 match）
  - `TurnOrchestrator`（建 engine + 注入依赖 + run_turn_streaming + 事件转发 + confirm channel + cancel）
  - `SessionRestorer`（把 `session_restore.rs` 移出二进制）
  - `bootstrap_providers` / `setup_tooling` / `configure_permissions` 作为 lib 函数
- TUI 与桌面都改为依赖 `yode-runtime`，各自只保留薄 UI 映射
- 治理 `yode-core/src/setup.rs` 的 crossterm 依赖（移出 core）

**阶段 2 — 补桌面关键功能缺口（2-4 周，按优先级）**
1. **Slash 命令系统**（最高优先级，F8）：在 `Composer.tsx` 加 `/` 解析，至少先补 `/compact`、`/clear`、`/checkpoint`、`/rewind`、`/plan`、`/sessions`、`/resume`、`/cost`、`/help`、`/model`、`/effort`
2. **Updater apply**（B4）：把 `src/main.rs:132-160` 的 apply/重启逻辑下沉到 `yode-runtime`，或接 Tauri updater 插件
3. **Inspector**（F7）：迁移 TUI `/inspect` 工具调用检视为桌面侧边栏
4. **Worktree 创建/进入**：补 `worktree_create`，对齐 TUI `/coordinate`
5. **Memory 管理 UI**：`/memory` 前端化
6. **Dev 工作流**（`/review`/`/pipeline`/`/ship`/`/remote-control`/`/coordinate`/`/teams`）：评估保留为桌面功能还是定位 TUI-only

**阶段 3 — 双轨期管理（并行运行期）**
- 维持 `~/.yode/` 数据共享（已兼容）
- 文档明确：TUI 为 CLI/SSH/无 GUI 场景长期支持路径，桌面为主流 GUI 路径
- parity 基线继续以 TUI 为基准，桌面逐步加入对比

**阶段 4 — TUI 收缩与删除（远期，仅在桌面功能对齐后）**
- 当桌面覆盖 ≥90% TUI 命令且共享 runtime 层稳定后：
  - 删除 `crates/yode-tui`（68k 行）
  - 删除 `vendor/ratatui`（3.0M）
  - 从 workspace 移除 `yode-tui`/`ratatui`/`crossterm`(TUI 部分)
  - `src/main.rs` 改为默认启动桌面，或保留为纯 CLI 瘦入口
- **不建议在此之前提前删 TUI**——当前桌面缺 slash 命令、inspector、updater apply、dev 工作流，过早删除会丢功能

### 7.7 关键文件索引

| 关注点 | 路径 |
|---|---|
| CLI 入口（仍走 TUI） | `src/main.rs:342` |
| TUI 命令注册（52 命令） | `crates/yode-tui/src/commands/mod.rs:107-160` |
| TUI 引擎事件适配 | `crates/yode-tui/src/app/engine_events/streaming.rs` |
| 桌面 Tauri 命令表（55 个） | `apps/yode-desktop/src-tauri/src/lib.rs:536-593` |
| 桌面 runtime 主体 | `apps/yode-desktop/src-tauri/src/runtime.rs` |
| 桌面 turn 循环 + 事件翻译 | `runtime.rs:1430-1780` |
| IPC 协议 | `apps/yode-desktop/src-tauri/src/protocol.rs` |
| 前端事件订阅 | `apps/yode-desktop/src/App.tsx:875-945` |
| 前端 reducer | `apps/yode-desktop/src/components/timelineUtils.ts:945` |
| 共享引擎 | `crates/yode-core/src/engine/` |
| 共享数据层 | `crates/yode-core/src/db/`、`config.rs:228`、`session_memory.rs` |
| 共享层污染点 | `crates/yode-core/src/setup.rs:3` |
| CI | `.github/workflows/ci.yml:19` |
| Release（仅桌面） | `.github/workflows/release.yml` |

---

## 八、更新后的执行顺序

1. **快速止血**：R1–R6 纯重复消除（~510 行）、U9 死命令、A4 去 vendor ratatui、**B2 桌面 CI 门禁**、**F2 引入 zustand 替代三套真相源**
2. **健壮性 P0**：T1 锁中毒、T2 协调器静默失败、T3 阻塞 I/O
3. **体验 P0**：U1/F6 i18n 定调（双端统一）、U2 确认态留白、U3 主题系统
4. **桌面功能补齐**：F8 slash 命令系统、B4 updater apply、F7 inspector
5. **结构性重构**：A1/A2/F1 三端上帝对象拆分、A3 拆 crate、**阶段 1 抽 yode-runtime 共享层**、A5 统一轮次循环
6. **持续打磨**：U4 状态行分级、U5/F4 类型安全、F5 测试覆盖、T4 类型化错误、T9 文档、F17 流式 memoization

> 权限和 hook 子系统是设计良好的范本，重构 engine/app/runtime 时可参照其模块化边界。双轨期优先抽共享层再删 TUI，避免重复劳动与功能丢失。
