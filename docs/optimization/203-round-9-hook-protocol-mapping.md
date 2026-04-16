# Round 9 Hook Protocol Mapping

## Scope

这份文档对应 round-9 tracker 的 `021`，用于明确 `Yode` 当前 hook protocol 与 Claude Code 官方 hooks 文档之间的事件映射与剩余差距。

基线文档：

- [Claude Code Hooks](https://docs.anthropic.com/en/docs/claude-code/hooks)

## Current Yode Hook Events

`Yode` 当前 hook event 集合：

- `session_start`
- `session_end`
- `pre_turn`
- `pre_compact`
- `post_compact`
- `pre_tool_use`
- `post_tool_use`
- `post_tool_use_failure`
- `permission_request`
- `permission_denied`
- `user_prompt_submit`
- `context_compressed`
- `subagent_start`
- `subagent_stop`
- `task_created`
- `task_completed`
- `worktree_create`

## Mapping

Claude Code -> Yode:

- `PreToolUse` -> `pre_tool_use`
- `PostToolUse` -> `post_tool_use`
- `Notification` wake path -> `wakeNotification` in hook output
- `SessionStart` / `SessionEnd` -> `session_start` / `session_end`
- `SubagentStart` / `SubagentStop` -> `subagent_start` / `subagent_stop`
- `TaskCreated` / `TaskCompleted` -> `task_created` / `task_completed`
- `WorktreeCreate` -> `worktree_create`

Yode-only or Yode-biased events:

- `pre_turn`
- `pre_compact`
- `post_compact`
- `permission_request`
- `permission_denied`
- `user_prompt_submit`
- `context_compressed`

## Protocol Notes

- `Yode` 现在支持 `decision: "defer"`，会把 tool call 落成 hook deferred artifact，而不是直接视为 error。
- `Yode` 仍没有 Claude Code 文档里更强的外部 resume/control-plane 协议；当前 defer 更偏本地 artifact-backed continuation。
- `task_created/task_completed` 目前主要覆盖 sub-agent background task 路径，而不是所有 runtime task source。

## Conclusion

- round-9 之前，`Yode` 的 hook protocol 主要集中在 tool/permission。
- 现在已经扩到 sub-agent / task / worktree lifecycle，协议面已经明显接近 Claude Code 官方 hooks 基线。
- 剩余差距主要在真正的 external control-plane resume semantics，而不是事件名缺失。
