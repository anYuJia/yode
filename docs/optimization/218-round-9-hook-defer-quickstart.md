# Round 9 Hook Defer Quickstart

1. 触发一个会进入 defer path 的 tool call。
2. 用 `/inspect artifact latest-hook-deferred` 查看 defer 摘要。
3. 用 `/inspect artifact latest-hook-deferred-state` 查看具体 state。
4. 用 `/diagnostics` 和 `/status` 交叉确认 hook、permission、runtime family 的最新状态。
5. 继续执行前先判断是恢复、重试还是人工接管。
