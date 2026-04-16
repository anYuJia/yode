# Round 9 Claude Settings Recheck

## Baseline

复核日期：`2026-04-16`

对照官方页面：

- `https://code.claude.com/docs/en/settings`

## Key Observation

- Claude Code 当前文档继续强调 `Managed / User / Project / Local` scope 与 precedence。
- 同时明确存在 server-managed、MDM / OS-level managed delivery，以及 `allowManagedHooksOnly`、`allowManagedMcpServersOnly`、`allowManagedPermissionRulesOnly` 这类 managed-only enforcement 开关。

## Parity Read

- `Yode` 已有 layered settings visibility、managed MCP inventory、permission governance artifact 与 scope diagnostics。
- 剩余差距是没有 Claude 那种真正的 server-managed / OS policy delivery，也缺少这些 managed-only enforcement flags 的严格产品化实现。
