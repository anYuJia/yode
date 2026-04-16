# Round 9 Hook Parity Closeout

## Scope

这份文档对应 round-9 tracker 的 `030`，记录 `021-029` 完成后的 hook parity 收口状态。

## Closed

- hook protocol mapping against Claude Code official hooks
- new lifecycle events:
  - `subagent_start`
  - `subagent_stop`
  - `task_created`
  - `task_completed`
  - `worktree_create`
- `pre_tool_use` defer semantics
- deferred tool call artifact/state
- richer defer metadata snapshots
- hook defer inspect aliases and hook artifact history family
- hook protocol verification tests

## What Changed

- hook output 现在不只支持 `block`，还支持 `defer`
- defer 不再是“给用户一段提示文字”，而是会落成可 inspect 的 state/artifact
- sub-agent、runtime task、worktree creation 都已经进入 hook event model，而不是只存在于 runtime state 或工具 metadata 里
- hook artifacts 不再只有 failure markdown，deferred hook state 也进入 artifact inventory / summary / timeline / inspect

## Residual Gaps

- task lifecycle hook 目前仍偏向 sub-agent background task，尚未覆盖所有 runtime task source
- defer 还是本地 artifact-backed continuation，不是 Claude Code 式外部 resume plane
- 缺少更完整的 hook operator guide 与 richer lifecycle dashboards

## Conclusion

- `Yode` 的 hook protocol 已经从“tool/permission hook”推进到“lifecycle hook protocol”。
- 相对 Claude Code，hook 面的主要差距已经不再是事件名缺失，而是 external orchestration depth。
