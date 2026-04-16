# Round 9 Claude Permission Modes Recheck

## Baseline

复核日期：`2026-04-16`

对照官方页面：

- `https://code.claude.com/docs/en/permission-modes`

## Key Observation

- Claude Code 当前文档写明：会话内 `Shift+Tab` 默认循环 `default -> acceptEdits -> plan`；`auto` 与 `bypassPermissions` 需要先启用后才进入 cycle；`dontAsk` 永远不进入 cycle。
- Web/cloud session 与 Remote Control session 暴露的 mode 子集也不同。

## Parity Read

- `Yode` 现已具备 `default / plan / auto / accept-edits / bypass` 五态语义，并给 `/permissions mode` 增加了 operator guide。
- 剩余差距是 TUI cycle 仍然采用 `default -> auto -> plan`，且没有单独的 `dontAsk` 产品名义与远端会话模式差异化。
