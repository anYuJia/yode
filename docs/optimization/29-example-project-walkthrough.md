# Example Project Walkthrough

## Scenario

用户要求：

> 修复一个长会话里的编译问题，并保留 compaction / memory / transcript / task artifacts

## Flow

1. Agent 读取文件并调用工具
2. `/status` 查看 compact / memory / recovery / tool runtime
3. `/tools` 检查最近一轮 tool trace 与 tool artifact
4. `/permissions` 查看 deny history 与 classifier explanation
5. `/tasks` 查看 background bash / agent 状态
6. `/memory latest` / `/memory compare latest latest-1` 查看 transcript 变化
7. `/diagnostics` 做总览检查

## Expected artifacts

- compact 后写 `session.md` 与 transcript
- live refresh 写 `session.live.md`
- 每轮工具执行写 `./.yode/tools/*`
- background task 写 `./.yode/tasks/*`
