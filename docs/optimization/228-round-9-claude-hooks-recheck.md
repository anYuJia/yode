# Round 9 Claude Hooks Recheck

## Baseline

复核日期：`2026-04-16`

对照官方页面：

- `https://code.claude.com/docs/en/hooks`

## Key Observation

- Claude Code hooks 参考页当前覆盖了大量 lifecycle event：`PreToolUse`、`PostToolUse`、`PermissionRequest`、`SubagentStart/Stop`、`TaskCreated/Completed`、`WorktreeCreate/Remove`、`ConfigChange`、`FileChanged`、`SessionEnd` 等。
- `defer` 语义当前被明确定义为主要面向 `claude -p` / SDK / 自定义 UI 的 non-interactive 单工具调用恢复流。

## Parity Read

- `Yode` 已补上 defer、sub-agent/task/worktree lifecycle 以及 inspectable defer artifact/state。
- 剩余差距是没有 Claude 文档里的 HTTP hooks / prompt hooks / agent hooks product plane，也没有完全对齐其“仅单工具调用可 defer”的约束与恢复协议。
