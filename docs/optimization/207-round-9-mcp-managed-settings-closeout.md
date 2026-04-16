# Round 9 MCP Managed Settings Closeout

## Scope

这份文档对应 round-9 tracker 的 `051-060`，记录 MCP 与 managed settings control-plane 这一批的收口状态。

## Closed

- startup artifacts:
  - `settings-scopes.json`
  - `managed-mcp-inventory.json`
- startup artifact parsers for settings scopes / managed MCP inventory
- `inspect` aliases:
  - `latest-settings-scopes`
  - `latest-managed-mcp-inventory`
  - `latest-permission-policy`
- `/mcp` 输出接入 settings scopes / managed MCP inventory / remediation hints
- `/status` artifact section接入 settings scopes 与 managed MCP inventory
- startup/export candidate list 纳入新 artifact family

## What Changed

- MCP 控制面不再只有“当前连上了哪些 server / tools”，还会显示这些 server 来自哪个 settings scope
- managed / user / project / local 配置层已经开始进入 startup artifact 体系，而不是只存在于启动逻辑内部
- MCP failure / reconnect 的 remediation 现在可以直接跳到对应 artifact，而不是纯粹口头提示

## Residual Gaps

- 还没有真正的 enterprise-managed remote policy distribution plane
- settings scope 仍是 file-backed artifact view，不是独立 settings workspace
- managed MCP inventory 目前偏 startup snapshot，还不是 live session policy reconciler

## Conclusion

- `Yode` 的 MCP 面已经从“工具注册状态”推进到了“settings-scoped control plane visibility”。
- 相对 Claude Code，剩余差距主要在更深的管理平面与实时策略同步，而不是缺少基础 artifact/control-plane primitives。
