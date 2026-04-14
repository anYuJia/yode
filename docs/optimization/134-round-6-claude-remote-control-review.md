# Round 6 Claude Remote Control Review

## Scope

这份文档对应 round-6 tracker 的 `063`。基线参考：

- `https://code.claude.com/docs/en/remote-control`

## Claude Baseline

- Claude Remote Control 允许从 CLI、已有 session、VS Code 启动远程控制，并在 browser / mobile / terminal 间同步同一会话。
- 它支持 server mode、session naming、capacity、spawn mode、connection recovery，以及同一会话多端交互。

## Yode Now

- Yode 已有 `/remote-control plan|latest|queue|doctor|bundle`，并能把 remote control session / queue / handoff 落成 artifact。
- 这些 artifact 已经能和 checkpoint、orchestration、remote execution evidence 链接。

## Gap

- Yode 还没有 live multi-device session，不存在真正的 remote transport / reconnect / synchronized conversation。
- Claude 的 Remote Control 是运行时能力；Yode 目前只是 control-plane planning surface。
