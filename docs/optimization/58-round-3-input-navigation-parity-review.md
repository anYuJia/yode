# Round 3 Input And Navigation Parity Review

## Scope

这份文档对应 round-3 tracker 的 `090`，总结 `081-089` 完成后 input/navigation/layout polish 的状态。

Claude 参考：

- `claude-code-rev/src/screens/REPL.tsx`
- `claude-code-rev/src/commands/doctor/doctor.tsx`

当前 `yode`：

- `crates/yode-tui/src/ui/panels.rs`
- `crates/yode-tui/src/ui/responsive.rs`
- `crates/yode-tui/src/ui/tool_confirm.rs`
- `crates/yode-tui/src/ui/wizard.rs`
- `crates/yode-tui/src/ui/status_bar.rs`

## What Closed In 081-089

- panel primitives 现在已经具备 keyhint bar、search prompt label、focus state、scroll sync helper、empty-state copy 和 narrow-density fallback rect。
- wizard / confirm 这两个最接近“inspector mode”的界面已经切到同一套 panel helper 上。
- status bar 现在有显式的 narrow-width collapse rule，而不是散落在各个 `if width < ...` 分支里。
- panel fallback 在窄终端下会退回 full-width，而不是继续硬居中挤压内容。

## Conclusion

- 这批优化把 `yode` 的 input/navigation 结构从“各个界面自己拼 keyhint/footer”推进到了“有共享 panel/navigation 原语”。
- 真正还没做完的，是把这些 primitive 继续扩展到 transcript/review/task inspector 的交互式 workspace。
