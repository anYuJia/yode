# Round 3 Task Workspace Parity Review

## Scope

这份文档对应 round-3 tracker 的 `020`，目标是对照 Claude Code Rev 当前的 task shell 体验，评估 `yode` 在 task workspace 这条线上补完 `011-019` 之后的状态。

Claude 参考：

- `claude-code-rev/src/QueryEngine.ts`
- `claude-code-rev/src/services/tools/toolOrchestration.ts`

当前 `yode`：

- `crates/yode-tui/src/commands/info/tasks.rs`
- `crates/yode-tui/src/commands/info/tasks_render.rs`
- `crates/yode-tui/src/commands/info/tasks_helpers.rs`
- `crates/yode-tui/src/runtime_timeline.rs`
- `crates/yode-tui/src/runtime_artifacts.rs`

## What Closed In 011-019

- `/tasks` list 现在按 freshest activity 排序，并按 `source_tool` 分组，不再只是平铺一串 task id。
- task detail 现在有 timeline、retry chain、failure summary、artifact backlinks、transcript preview、recent progress 和 output tail。
- `/tasks read` 已改成更像 pager-friendly 的分段输出，而不是只打印最后几十行。
- cancel 响应现在会带上当前状态、retry chain 和 artifact backlink，而不是只有一句“requested”。
- runtime timeline 已能把 task transition 和 runtime/doctor surfaces 对齐起来。

## Comparison

### Strengths

- `yode` 现在已经具备足够的 CLI task workspace：list、detail、read、follow、stop 和 artifact/timeline 之间有稳定互链。
- 对排障来说，retry chain、source_tool grouping 和 transcript preview 是最关键的增益点。
- 相比 Claude 的 richer task shell，`yode` 的优势仍然是文本输出清晰、可导出、容易 copy/paste 进 issue 或 review。

### Remaining Gaps

- 仍然没有真正的 panelized task inspector，也没有 split-pane/pager 交互。
- task timeline 还是文本块，不是可跳转、可展开的时间线 widget。
- task output 还没有键盘驱动的浏览/定位能力，只是 pager-friendly formatting。
- Claude 那种更深的 orchestration shell 和 task-to-tool UI linkage 仍然更重。

## Conclusion

- 对 CLI parity 来说，`yode` 的 task workspace 已从“能看 task”提升到“能系统排查 task”。
- 继续往前走时，不应该再重复堆文本字段，而应该直接转向 panel/pager primitives，让 task workspace 真正变成一个操作台。
