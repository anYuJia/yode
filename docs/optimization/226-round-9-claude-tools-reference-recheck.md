# Round 9 Claude Tools Reference Recheck

## Baseline

复核日期：`2026-04-16`

对照官方页面：

- `https://code.claude.com/docs/en/tools-reference`

## Key Observation

- Claude Code 官方工具面明确列出 `Agent`、`AskUserQuestion`、`Bash`、`Cron*`、`Edit`、`EnterPlanMode`、`ExitPlanMode`、`EnterWorktree` 等一等工具。
- 同一页面还单列 `Monitor`，强调 Claude 可以在不中断当前会话的情况下持续观察后台变化并在事件发生时主动插话。

## Parity Read

- `Yode` 现在已有 `agent`、team runtime、workflow、review pipeline、remote runtime tools、plan mode tools。
- 剩余明显差距是没有 Claude 那种真正持续运行、可主动回报事件的 model-native `Monitor` tool 语义；当前更偏 `/tasks monitor` 与 artifact-backed follow。
