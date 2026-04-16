# Round 9 Tool Capability Taxonomy

## Taxonomy

- `read`
- `write`
- `shell`
- `remote`
- `background`
- `team`
- `mcp`
- `workflow`
- `general`

## Current Surface

- `tool_categories()` 已覆盖 remote/team/background 扩展分类
- `/tools list` 与 `/tools verbose` 现在同时显示 policy 与 taxonomy
- permission rules 可以继续沿用 category 维度做 allow/ask/deny

## Why It Matters

- 让 permission governance、tool inventory、operator diagnostics 共享同一组能力词汇
- 把 “这是个写工具” 和 “这是个 remote/background/team 工具” 明确分开
