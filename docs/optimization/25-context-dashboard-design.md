# Context Dashboard Design

## Goal

把 context / compact / memory / recovery / tools 的运行时状态统一暴露到：

- `/status`
- `/context`
- `/doctor`
- `/diagnostics`
- TUI status line

## Runtime slices

- Compact: count, auto/manual split, breaker reason
- Memory: live-memory 状态、最近更新时间、artifact 路径
- Recovery: current state、failed signature、permission explanation
- Tools: budget、progress、parallel、truncation、artifact
- Hooks: totals、timeouts、wake notifications

## UI rule

- `/status` 给完整诊断
- `/diagnostics` 给 overview
- TUI status line 只放 compact/memory/recovery 的短 indicator

## Tradeoff

- 不把全部细节都塞进 TUI
- 详细 trace 交给 `/tools` / `/permissions` / `/tasks`
