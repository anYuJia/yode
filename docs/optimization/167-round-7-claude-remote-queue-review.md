# Round 7 Claude Remote Queue Review

## Scope

这份文档对应 round-7 tracker 的 `063`。基线参考：

- `https://code.claude.com/docs/en/remote-control`
- `https://code.claude.com/docs/en/claude-code-on-the-web`

## Claude Baseline

- Claude Remote Control 本质是 live synced session；cloud sessions 支持 `/tasks`、teleport、parallel runs。
- 队列和任务 continuation 由真实 remote session 驱动，而不是 artifact queue。

## Yode Now

- Yode 已有 remote queue item status、run/retry/ack、queue execution artifact、task handoff artifact。

## Gap

- queue 仍是本地 command bridge，不是真 remote transport。
- Claude 的 remote continuation 已是 live execution plane；Yode 还在 control-plane runtime。
