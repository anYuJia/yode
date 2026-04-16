# Round 9 Managed Settings Rollout Note

## What Changed

- settings scope 从单层配置视图升级为 `managed / user / project / local / session / cli` precedence surface
- managed MCP inventory、settings scopes、permission governance 都变成 inspectable startup/runtime artifact
- `/permissions governance`、`/mcp`、`/status`、`/inspect artifact latest-settings-scopes` 构成 operator 面

## Rollout Advice

1. 先只启用 managed visibility，不立刻强推 deny 规则。
2. 观察 `/inspect artifact latest-settings-scopes` 与 `/inspect artifact latest-managed-mcp-inventory`。
3. 再逐步收紧 `managed` scope 的 permission rules。
4. 最后把 project/local overrides 与 managed precedence 一起审查。

## Risk

- 规则源增多后，如果没有看 `precedence chain`，很容易误判生效来源。
