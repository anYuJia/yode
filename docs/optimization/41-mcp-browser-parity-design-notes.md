# MCP / Browser Parity Design Notes

## Goal

对齐 Claude Code 在 MCP 与 browser-access 相关诊断面的几个关键体验：

- server 级健康/认证/缓存/延迟状态可见
- browser / MCP 入口的职责边界清晰
- 出错时能给出“下一步该做什么”的操作提示，而不是只有失败文本

## Current Yode Baseline

当前已经具备：

- `/mcp` server summary
- `/mcp` auth readiness summary
- MCP resource cache stats
- MCP tool latency telemetry
- MCP reconnect/backoff diagnostics
- `mcp_auth`, `list_mcp_resources`, `read_mcp_resource` 工具入口

## Remaining Gaps

1. browser-like authenticated flows 还没有统一的状态模型，`mcp_auth` 仍是轻量占位入口。
2. MCP server connect / reconnect 生命周期没有落成完整状态机，只能提供 attempts/failures/backoff 建议。
3. resource cache 目前是进程内缓存，没有 TTL、server invalidation、显式清理命令。
4. `/mcp` 还缺少“server detail view”，现在更偏 summary 卡片。
5. browser 访问失败和 MCP 认证失败还没有统一归因分类。

## Recommended Next Steps

1. 给 MCP server 引入显式 runtime state：`connecting | ready | degraded | auth_required | failed`。
2. 把 `mcp_auth` 从静态 URL 生成器升级成 provider-backed flow，并把结果回写到 `/mcp` 状态。
3. 给 resource cache 增加 TTL、last refresh timestamp、per-server invalidation。
4. 在 `/mcp <server>` 明细页中展示 auth、latency、cache、reconnect 历史。
5. 为 browser-only 路径写一份 capability matrix，明确哪些场景应该优先走 browser，哪些应该优先走 MCP。

## Design Boundary

- MCP 负责结构化工具、资源、认证与 server 诊断。
- browser 路径负责页面抓取、截图、交互式浏览。
- 对同一目标既可 MCP 又可 browser 时，优先选择结构化、可缓存、可审计的一侧；只有在 MCP 无法覆盖交互细节时才退到 browser。
