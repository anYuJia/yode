# Round 9 Sub-Agent Team Closeout

## Scope

这份文档对应 round-9 tracker 的 `031-040`，记录 sub-agent / team runtime 这一批的收口状态。

## Closed

- agent team artifact family
  - `agent-team.md`
  - `agent-team-state.json`
  - `agent-team-messages.md`
  - `agent-team-monitor.md`
  - `agent-team-bundle.md`
- first-class tools
  - `team_create`
  - `send_message`
  - `team_monitor`
- `agent` / `coordinate_agents` 自动接入 team runtime artifacts
- background sub-agent status now backfills team member state
- permission inheritance is explicitly persisted per team member
- inspect aliases for latest team / monitor / bundle / subagent result
- sub-agent lifecycle runtime hooks already surface into the broader hook protocol

## What Changed

- `Yode` 现在不再只有“单个 sub-agent result markdown”；它已经有面向多成员协作的 team state
- `coordinate_agents` 不再只是 coordinator summary/state，它会同时生成 team runtime artifacts
- `agent` 在给定 `team_id/member_id` 时会自动把结果、artifact path、runtime task id 回写到 team state
- `send_message` 和 `team_monitor` 让 team runtime 不再只是后台文件，而成为可以直接调用的一等工具

## Residual Gaps

- 还没有真正的 long-lived team session control plane
- `send_message` 目前是 artifact-backed team mailbox，不是 live cross-agent messaging bus
- TUI 侧还没有独立 `/teams` 命令面，主要通过 inspect aliases 使用

## Conclusion

- round-9 把 `Yode` 的 sub-agent 能力从“单次调用 primitive”推进到了“artifact-backed team runtime”。
- 相对 Claude Code，剩余差距现在主要在实时协作深度，而不是 team primitive 的缺失。
