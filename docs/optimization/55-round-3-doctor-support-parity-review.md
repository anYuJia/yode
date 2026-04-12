# Round 3 Doctor And Support Parity Review

## Scope

这份文档对应 round-3 tracker 的 `060`，用于总结 `051-059` 完成后的 doctor / support bundle 状态。

Claude 参考：

- `claude-code-rev/src/commands/doctor/doctor.tsx`
- `claude-code-rev/src/utils/doctorDiagnostic.ts`
- `claude-code-rev/src/utils/debug.ts`

当前 `yode`：

- `crates/yode-tui/src/commands/info/doctor/report/mod.rs`
- `crates/yode-tui/src/commands/info/doctor/report/shared.rs`
- `crates/yode-tui/src/commands/info/doctor/report/local.rs`
- `crates/yode-tui/src/runtime_artifacts.rs`

## What Closed In 051-059

- doctor bundle 现在不只是导出四个文本文件，还会生成 `bundle-manifest.json`、`bundle-overview.txt`、`support-handoff.md`。
- overview 会统计各报告的 severity 分布，并给出 artifact freshness 摘要。
- bundle 会把最新 runtime timeline 和 hook failure inspector 一起带上。
- copy/paste summary 现在会直接列出 bundle 目录和主要文件，便于 issue / chat 里转交。
- shared checklist helper 让 support handoff 和 bundle overview 复用了同一份报告清单。

## Comparison

### Strengths

- `yode` 的 support bundle 已经从“导出若干文本”升级成“可转交、可索引、可追踪 freshness”的调试材料。
- runtime timeline / hook inspector 被纳入 doctor bundle 后，排查跨度更接近 Claude 的 support/debug 流程。
- 对 CLI 使用场景来说，当前 bundle 已经足够支撑 issue triage 和异步 handoff。

### Remaining Gaps

- 仍然没有更重的交互式 doctor dashboard，只是 bundle 和文本 overview。
- support bundle 目前偏本地文件导出，没有进一步的 upload/share 工作流。
- Claude 在 richer debug workflow 和 UI guidance 上仍然更强。

## Conclusion

- 这一轮后，`yode` 的 doctor/support 层已经完成最关键的 artifact 化和 handoff 化。
- 后续如果继续推进，应该转向 remote/browser workflow 本身，而不是继续堆 doctor 文本字段。
