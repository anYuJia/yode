# Round 4 Workspace Navigation Parity Review

## Scope

这份文档对应 round-4 tracker 的 `019`，用于总结 workspace navigation helper 与 jump/completion 体系落地后的状态。

当前 `yode`：

- `crates/yode-tui/src/commands/workspace_nav.rs`
- `crates/yode-tui/src/commands/context.rs`
- `crates/yode-tui/src/commands/info/memory.rs`
- `crates/yode-tui/src/commands/dev/reviews.rs`

## What Closed In 011-018

- workspace 现在已经有 jump inventory、breadcrumb、selection summary、compact path badge、stale artifact banner。
- `CompletionContext` 现在携带 `working_dir`，使 `/memory`、`/reviews` 可以给出基于真实 artifact 的动态 completion。
- task / transcript / review / runtime artifact 都已有共享 jump target helper，而不是各自硬编码 footer 文案。

## Conclusion

- 这批改动让 round-4 真正开始从“workspace 只是输出长得像”过渡到“workspace 有统一导航语义”。
- 真正还没做完的是 interactive inspector 和 command palette 更深的状态联动。
