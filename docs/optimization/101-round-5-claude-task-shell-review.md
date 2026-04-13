# Round 5 Claude Task Shell Review

## Scope

这份文档对应 round-5 tracker 的 `062`。基线参考：

- `https://code.claude.com/docs/en/commands`
- `https://code.claude.com/docs/en/claude-code-on-the-web`
- `https://code.claude.com/docs/en/remote-control`

## Claude Baseline

- Claude Code 官方命令面已经有 `/tasks`、`--remote`、`/remote-control`、`/schedule`、`/loop`。
- cloud web session、remote control、本地 CLI 之间的任务形态是明确区分的，而且支持 parallel remote tasks 与 session handoff。

## Yode Now

- Yode 已经有 `/tasks`、workflow/coordinator artifact、runtime orchestration timeline、remote capability artifact。
- export/status/inspect 已经能把本地 orchestration 状态串成一个 operator-facing shell。

## Gap

- Yode 还没有云端 task shell，也没有 Remote Control 那种跨设备 session continuation。
- workflow/coordinator 还没有真正的 execution engine，只是在 prompt + artifact 层模拟 task shell。

## Conclusion

- round-5 把 Yode 的 task shell 从纯 runtime tasks 扩到了 orchestration/runtime artifacts。
- 对标 Claude，下一步缺的已经是 remote execution plane，而不是 task list 文本本身。
