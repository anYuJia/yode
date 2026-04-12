# Round 3 Recovery And Permission Parity Review

## Scope

这份文档对应 round-3 tracker 的 `050`，用于总结 `041-049` 落地后的 permission / recovery / hook UX 状态。

Claude 参考：

- `claude-code-rev/src/tools.ts`
- `claude-code-rev/src/services/tools/toolOrchestration.ts`
- `claude-code-rev/src/utils/doctorDiagnostic.ts`

当前 `yode`：

- `crates/yode-tui/src/commands/info/runtime_inspectors.rs`
- `crates/yode-tui/src/commands/info/hooks_cmd.rs`
- `crates/yode-tui/src/commands/tools/permissions.rs`
- `crates/yode-tui/src/runtime_timeline.rs`
- `crates/yode-tui/src/runtime_artifacts.rs`

## What Closed In 041-049

- hook failure / timeout 现在有统一 summary helper，也能落成独立 inspector artifact。
- `/hooks` 不再只是几个计数，而是能直接指向 hook failure artifact 并给出 preview。
- `/status` artifacts 区里加入了 hook inspector backlink。
- permission artifact 现在有 preview helper，`/permissions` 能直接看到最近一次 decision artifact 摘要。
- repeated denial / confirmation suggestions 被压成 recovery hint，而不是散乱地堆行。
- runtime timeline 的 recovery entry 已经合并了 breadcrumb 摘要，不再只有 state 名字。
- `/brief` 现在会直接带 recovery artifact preview。
- rules snapshot 现在有 compact diff summary，可以快速判断 session/user/project/cli 规则源的构成。

## Comparison

### Strengths

- `yode` 在 CLI 场景里已经做到了 permission / recovery / hook 三个面之间的互链。
- 最近一次 permission 决策、recovery artifact、hook failure inspector 都能被 status/brief/doctor 命中。
- 与 Claude 的主要差距不再是“为什么失败/为什么 ask 看不到”，而是更深的 interactive UI shell。

### Remaining Gaps

- 仍然缺少 panelized inspector，让 permission / recovery / hook detail 以统一面板出现。
- permission artifact 仍主要是 raw JSON preview，而不是结构化 rule diff viewer。
- recovery state 还没有和 transcript/review workspace 做双向跳转。

## Conclusion

- 这一轮后，`yode` 的 permission / recovery / hook UX 已经从“有诊断字段”提升为“有实际 inspector”。
- 后续如果继续推进，重点应该转向 inspector 面板化，以及和 transcript/review workspace 的联动，而不是继续堆更多字符串字段。
