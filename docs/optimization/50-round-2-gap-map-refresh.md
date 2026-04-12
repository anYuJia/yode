# Round 2 Gap Map Refresh

## Scope

这份文档对应 round-2 tracker 的 `081-090`，目标是把本轮后半段的 Claude 对照结论固定下来，并刷新“还剩哪些真正的产品差距”。

## 081 Startup Artifact Bundle vs Claude Flow Notes

Claude 参考：

- `claude-code-rev/src/main.tsx`
- `claude-code-rev/src/QueryEngine.ts`

当前 `yode`：

- `src/main.rs`
- `src/app_bootstrap/artifacts.rs`
- `src/app_bootstrap/startup_summary.rs`
- `crates/yode-tui/src/commands/info/startup_artifacts.rs`

结论：

- `yode` 已经把 startup profile、tooling inventory、provider inventory、MCP failure summary、permission policy、bundle manifest 组装成稳定 artifact bundle。
- Claude 仍然有更重的前端状态机和 feature-flag 分支，但 `yode` 在 CLI 侧已经具备足够的启动证据链。

## 082 Tool Pool Diagnostics vs Claude UI Expectations

Claude 参考：

- `claude-code-rev/src/tools.ts`
- `claude-code-rev/src/utils/toolSearch.ts`

当前 `yode`：

- `crates/yode-core/src/engine/tool_pool_runtime.rs`
- `crates/yode-core/src/tool_runtime.rs`
- `crates/yode-tui/src/commands/info/status.rs`
- `crates/yode-tui/src/commands/info/status/render.rs`

结论：

- `yode` 已经能同时展示 active/deferred pool、tool_search 启用原因、permission state、matched rule 和 duplicate registration 诊断。
- 和 Claude 的主要差距不再是“看不到工具池状态”，而是缺少更重的面板式交互壳。

## 083 Permission Guidance vs Claude Prompt Language

Claude 参考：

- `claude-code-rev/src/tools.ts`
- `claude-code-rev/src/services/tools/toolOrchestration.ts`

当前 `yode`：

- `crates/yode-core/src/permission/mod.rs`
- `crates/yode-core/src/permission/tests.rs`
- `crates/yode-tui/src/commands/tools/permissions.rs`
- `crates/yode-tui/src/runtime_display.rs`

结论：

- `yode` 已补齐 classifier rationale、pattern-match reason、recent denials、denial prefixes、repeated confirmation suggestions。
- Claude 的 prompt 文案上下文仍更细，但 `yode` 已经把“为什么 ask/deny”和“下一步如何配置规则”说清楚。

## 084 Resume Telemetry vs Claude Session Storage Views

Claude 参考：

- `claude-code-rev/src/utils/sessionStorage.ts`

当前 `yode`：

- `src/app_bootstrap/session_restore.rs`
- `src/app_bootstrap/startup_summary.rs`
- `crates/yode-tui/src/commands/info/memory/transcripts/metadata_runtime.rs`
- `crates/yode-tui/src/commands/info/memory/render.rs`

结论：

- `yode` 已经把 metadata lookup、full restore、decode/skipped、fallback reason、resume warmup 都做成可见 telemetry。
- Claude 仍偏客户端存储视图，`yode` 仍偏 DB + artifact，但恢复可诊断性已经对齐。

## 085 Background Task Detail Views vs Claude Task UX

Claude 参考：

- `claude-code-rev/src/QueryEngine.ts`
- `claude-code-rev/src/services/tools/toolOrchestration.ts`

当前 `yode`：

- `crates/yode-tui/src/commands/info/tasks.rs`
- `crates/yode-tui/src/commands/info/tasks_render.rs`
- `crates/yode-tui/src/runtime_artifacts.rs`
- `crates/yode-tui/src/commands/info/status.rs`

结论：

- `yode` 已具备 task list、task detail、output preview、progress history、runtime task inventory artifact 和 status backlink。
- Claude 仍然有更完整的 task shell，但对 CLI 排障来说，`yode` 的任务细节面已够用。

## 086 Doctor Bundle Output vs Claude Support / Debug Needs

Claude 参考：

- `claude-code-rev/src/commands/doctor/doctor.tsx`
- `claude-code-rev/src/utils/doctorDiagnostic.ts`
- `claude-code-rev/src/utils/doctorContextWarnings.ts`

当前 `yode`：

- `crates/yode-tui/src/commands/info/doctor/report/mod.rs`
- `crates/yode-tui/src/commands/info/doctor/report/local.rs`
- `crates/yode-tui/src/commands/info/doctor/report/remote.rs`
- `crates/yode-tui/src/commands/utility/export.rs`

结论：

- `yode` 的 doctor 已能导出 local / remote-env / remote-review / remote-artifacts 四件套，并能被 diagnostics export bundle 复用。
- Claude 的 support/debug 仍更偏 UI 流程化，但 `yode` 已有可打包、可转交的支持材料。

## 087 Narrow-Width Input Layout vs Claude REPL

Claude 参考：

- `claude-code-rev/src/screens/REPL.tsx`

当前 `yode`：

- `crates/yode-tui/src/ui/layout.rs`
- `crates/yode-tui/src/ui/responsive.rs`
- `crates/yode-tui/src/ui/input/render.rs`
- `crates/yode-tui/src/ui/input/wrapping.rs`

结论：

- `yode` 已经把 narrow-width 的 input wrapping、cursor tracking、history search、file popup 路径拆开，布局行为稳定很多。
- Claude 仍有更复杂的 REPL widget 体系，但这一项已不再构成结构性差距。

## 088 Tool Result Folding vs Claude Transcript Readability

Claude 参考：

- `claude-code-rev/src/QueryEngine.ts`
- `claude-code-rev/src/services/tools/toolOrchestration.ts`

当前 `yode`：

- `crates/yode-tui/src/ui/chat_entries/folding.rs`
- `crates/yode-tui/src/ui/chat_entries/tools.rs`
- `crates/yode-tui/src/app/scrollback/entry_formatting/roles/tool_calls.rs`

结论：

- `yode` 已经把 tool result、bash preview、write/edit diff preview、subagent tool-call folding 都压到可读的 transcript 形态。
- Claude 在 transcript 细节和交互式展开上更成熟，但“长结果可读”这一层已经基本对齐。

## 089 Status Density vs Claude Compact Runtime Panel

Claude 参考：

- `claude-code-rev/src/QueryEngine.ts`
- `claude-code-rev/src/tools.ts`
- `claude-code-rev/src/utils/sessionStorage.ts`

当前 `yode`：

- `crates/yode-tui/src/commands/info/status.rs`
- `crates/yode-tui/src/commands/info/status/render.rs`
- `crates/yode-tui/src/commands/info/status/sections.rs`
- `crates/yode-tui/src/commands/info/brief.rs`

结论：

- `yode` 现在的 `/status` + `/brief` 已覆盖 compact/runtime/memory/recovery/tools/hooks/artifacts 多层摘要。
- 与 Claude 的差距更多是 UI 呈现密度和交互方式，而不是 runtime 面板字段缺失。

## 090 Gap Map Refresh After 50 More Items

### Current Strengths

- startup / provider / permission / resume / doctor / export / runtime artifact 全链路可观测
- tool pool、runtime task、compaction、prompt cache、system prompt、recovery 都已有稳定诊断面
- TUI 的 input / transcript / folding 结构已明显更可维护

### Remaining Gap Candidates

- 缺少 Claude 那类 panel / dialog / pager 驱动的前端壳
- task、doctor、transcript 仍以文本命令面为主，不是交互式工作台
- hook timeline 与 tool timeline 还没有统一视图
- browser / remote / authenticated workflow 仍偏诊断脚手架，不是完整产品能力

### Recommendation

下一轮如果还要继续，不应再以“补字段”为主，而应转向：

1. panelized TUI primitives
2. unified runtime timeline
3. browser / remote workflow depth
