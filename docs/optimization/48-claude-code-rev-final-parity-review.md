# Claude Code Rev Final Parity Review

## Scope

这份文档用于完成 tracker 中 `091-100` 这一组收尾项，直接对照 `/Users/pyu/code/yode/claude-code-rev` 与当前 `yode` 的状态，并记录 25 / 50 / 100 项阶段刷新结论。

结论先行：

- `yode` 已经补齐 Claude Code Rev 里最影响实际体验的链路：
  - startup profiling / warmup
  - tool pool gating / deferred activation / ToolSearch diagnostics
  - shell permission explain / denial clustering / repeated-confirm suggestion
  - provider streaming shared helpers
  - resume cache / restore telemetry
  - status / doctor / artifact cross-links
- 剩余差异主要是产品风格和交互取舍，不再是“缺失底层能力”。

## 091 Startup Sequence

Claude 参考：

- [main.tsx](/Users/pyu/code/yode/claude-code-rev/src/main.tsx)
- [QueryEngine.ts](/Users/pyu/code/yode/claude-code-rev/src/QueryEngine.ts)

当前 `yode`：

- [main.rs](/Users/pyu/code/yode/src/main.rs)
- [tooling.rs](/Users/pyu/code/yode/src/app_bootstrap/tooling.rs)
- [provider_bootstrap.rs](/Users/pyu/code/yode/src/provider_bootstrap.rs)

对比结论：

- `yode` 现在已经有阶段 profiler、tooling/provider/db overlap、resume warmup 统计、startup artifacts。
- 与 Claude 的差别不再是“完全缺观测”，而是 Claude 还有更多前端会话态和 feature-flag 分支。
- 对命令行 Rust 应用来说，`yode` 当前启动可观测性已经够用。

## 092 Tool Inventory Gating

Claude 参考：

- [tools.ts](/Users/pyu/code/yode/claude-code-rev/src/tools.ts)
- [toolSearch.ts](/Users/pyu/code/yode/claude-code-rev/src/utils/toolSearch.ts)

当前 `yode`：

- [registry.rs](/Users/pyu/code/yode/crates/yode-tools/src/registry.rs)
- [tool_pool_runtime.rs](/Users/pyu/code/yode/crates/yode-core/src/engine/tool_pool_runtime.rs)
- [tool_search/mod.rs](/Users/pyu/code/yode/crates/yode-tools/src/builtin/tool_search/mod.rs)

对比结论：

- `yode` 已具备 request 前 deny 过滤、active/deferred pool snapshot、deferred activation、duplicate registration diagnostics。
- Claude 仍然在客户端权限上下文与 UI store 交互上更复杂，但核心 gating 能力已经对齐。

## 093 Bash Permission Prompts

Claude 参考：

- [Tool.ts](/Users/pyu/code/yode/claude-code-rev/src/Tool.ts)
- [bashPermissions.ts](/Users/pyu/code/yode/claude-code-rev/src/tools/BashTool/bashPermissions.ts)

当前 `yode`：

- [bash.rs](/Users/pyu/code/yode/crates/yode-core/src/permission/bash.rs)
- [shell.rs](/Users/pyu/code/yode/crates/yode-core/src/permission/shell.rs)
- [permissions.rs](/Users/pyu/code/yode/crates/yode-tui/src/commands/tools/permissions.rs)

对比结论：

- `yode` 现在已经有 classifier rationale、pattern explain、deny prefix clustering、repeated confirmation suggestions。
- Claude 的 permission context 更重，但 `yode` 已经把“为什么拦/为什么 ask/下次怎么配规则”讲清楚了。

## 094 Resume Path And Storage

Claude 参考：

- [sessionStorage.ts](/Users/pyu/code/yode/claude-code-rev/src/utils/sessionStorage.ts)

当前 `yode`：

- [session_restore.rs](/Users/pyu/code/yode/src/app_bootstrap/session_restore.rs)
- [db/mod.rs](/Users/pyu/code/yode/crates/yode-core/src/db/mod.rs)
- [metadata_runtime.rs](/Users/pyu/code/yode/crates/yode-tui/src/commands/info/memory/transcripts/metadata_runtime.rs)

对比结论：

- `yode` 已拆成 metadata-only session lookup 和 full transcript restore 两段。
- cache hit/miss / invalidation / restore decode / fallback reason 都已经可观测。
- Claude 仍偏 JSONL/append-log 模式，`yode` 仍偏 SQLite + artifacts，但两者都已具备恢复诊断能力。

## 095 Prompt Input Ergonomics

Claude 参考：

- [REPL.tsx](/Users/pyu/code/yode/claude-code-rev/src/screens/REPL.tsx)

当前 `yode`：

- [mod.rs](/Users/pyu/code/yode/crates/yode-tui/src/ui/mod.rs)
- [layout.rs](/Users/pyu/code/yode/crates/yode-tui/src/ui/layout.rs)
- [input/render.rs](/Users/pyu/code/yode/crates/yode-tui/src/ui/input/render.rs)

对比结论：

- `yode` 已经把 viewport layout、responsive density、palette 拆出来，输入区和 completion 的布局耦合明显下降。
- Claude 在 prompt input 的 feature 丰富度仍更高，但 `yode` 的终端输入渲染结构已经不再是阻塞项。

## 096 Background Task UX

Claude 参考：

- [QueryEngine.ts](/Users/pyu/code/yode/claude-code-rev/src/QueryEngine.ts)
- [toolOrchestration.ts](/Users/pyu/code/yode/claude-code-rev/src/services/tools/toolOrchestration.ts)

当前 `yode`：

- [tasks.rs](/Users/pyu/code/yode/crates/yode-tui/src/commands/info/tasks.rs)
- [tasks_render.rs](/Users/pyu/code/yode/crates/yode-tui/src/commands/info/tasks_render.rs)
- [status.rs](/Users/pyu/code/yode/crates/yode-tui/src/commands/info/status.rs)

对比结论：

- `yode` 已经把 runtime task inventory artifact、`/tasks` renderer、status cross-links 接起来。
- Claude 还有更复杂的 UI task shell，但 `yode` 已满足排查和跟踪后台任务的核心需求。

## 097 Status And Doctor Diagnostics

Claude 参考：

- [tools.ts](/Users/pyu/code/yode/claude-code-rev/src/tools.ts)
- [toolSearch.ts](/Users/pyu/code/yode/claude-code-rev/src/utils/toolSearch.ts)
- [sessionStorage.ts](/Users/pyu/code/yode/claude-code-rev/src/utils/sessionStorage.ts)

当前 `yode`：

- [status.rs](/Users/pyu/code/yode/crates/yode-tui/src/commands/info/status.rs)
- [status/render.rs](/Users/pyu/code/yode/crates/yode-tui/src/commands/info/status/render.rs)
- [doctor/mod.rs](/Users/pyu/code/yode/crates/yode-tui/src/commands/info/doctor/mod.rs)
- [doctor/report/local.rs](/Users/pyu/code/yode/crates/yode-tui/src/commands/info/doctor/report/local.rs)

对比结论：

- `yode` 现在的 `/status` / `/doctor` 已经覆盖 startup、turn runtime、tool pool、permission、resume cache、artifact cross-links。
- 与 Claude 的主要区别变成 UI 形态，而不是诊断维度缺失。

## 098 Refresh After 25 Items

25 项阶段性刷新结论：

- 启动 profiling、tool inventory、bash policy helper 是第一批关键收益点。
- 当时最大的差距仍然是 tool pool gating 和 ToolSearch 行为。

## 099 Refresh After 50 Items

50 项阶段性刷新结论：

- `yode` 已经具备可观测的 startup / provider / tool pool / resume 诊断。
- 主要剩余差距转向 streaming runtime、prompt input 结构、artifact cross-links 和最终 parity 文档。

## 100 Final Refresh After 100 Items

100 项最终刷新结论：

- tracker 范围内的 100 项已经全部完成。
- `yode` 与 Claude Code Rev 的差距已从“底层能力空缺”收敛为“实现风格和产品取舍差异”。
- 后续如果还要继续优化，应该转向：
  - 真正的产品新能力
  - UI 交互细节
  - 特定 provider / workflow 的深度体验
