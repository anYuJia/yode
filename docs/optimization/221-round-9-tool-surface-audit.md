# Round 9 Tool Surface Audit

## Date

`2026-04-16`

## Tool-Native Families

- orchestration: `coordinate_agents`, `workflow_run`, `workflow_run_with_writes`
- review pipeline: `review_changes`, `review_pipeline`, `review_then_commit`
- sub-agent/team: `agent`, `team_create`, `send_message`, `team_monitor`, `verification_agent`
- runtime control: `task_output`, `tool_search`, `enter_plan_mode`, `exit_plan_mode`
- remote runtime: `remote_queue_dispatch`, `remote_queue_result`, `remote_transport_control`

## Command-Native Operator Surfaces

- `/inspect`, `/status`, `/brief`, `/diagnostics`
- `/remote-control plan|monitor|doctor|bundle|handoff`
- `/checkpoint ...`
- `/permissions ...`

## Audit Conclusion

- round-9 前半段的主要缺口是 remote queue/transport 仍然主要由 slash command 驱动。
- 当前收口后，真正仍属于 command-native 的主要是 operator workspace、bundle、checkpoint 和 inspection，而不是 runtime primitive 本身。
