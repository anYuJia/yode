# Round 3 MCP And Browser Runtime Parity Review

## Scope

这份文档对应 round-3 tracker 的 `080`，总结 `071-079` 完成后 MCP/browser runtime depth 的状态。

Claude 参考：

- `claude-code-rev/src/tools.ts`
- `claude-code-rev/src/utils/doctorDiagnostic.ts`
- `claude-code-rev/src/utils/debug.ts`

当前 `yode`：

- `crates/yode-tui/src/commands/tools/mcp.rs`
- `crates/yode-tui/src/commands/tools/mcp_workspace.rs`
- `crates/yode-tui/src/commands/info/doctor/report/mod.rs`

## What Closed In 071-079

- `/mcp` 现在不只给出平均延迟，还带上 compact sparkline、reconnect timeline、auth/session summary、browser/MCP capability merge summary。
- browser-access state snapshot artifact 会落到 `.yode/remote/` 并进入 doctor bundle。
- remote tool source badge 统一成 `[mcp]` / `[browser]` / `[local]`，输出语义更稳定。
- resource cache activity summary 和 reconnect timeline helper 已被抽成共享 runtime helper。

## Conclusion

- `yode` 在 CLI 场景里已经把 MCP/browser runtime 从“能看服务器和工具数”提升到“能看 auth/cache/latency/reconnect/capability 合成面”。
- 剩余的主要差距仍然是 browser-backed workflow 本身，而不是 runtime 诊断深度。
