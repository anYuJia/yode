# Release Notes: Memory / Diagnostics Line

## Highlights

- 新增工具运行时 budget / progress / parallel / truncation 遥测
- `/tools` 从简单注册表升级成诊断入口
- `/permissions` 暴露 deny history、规则诊断与 classifier explanation
- 新增 `/tasks` 与 `/diagnostics`
- background bash / background agent 接入统一 task runtime
- `/memory` 增加 `pick` 视图、failed quick jump 与大 compare 性能保护
- TUI 状态栏增加 recovery hint，compact/memory 事件支持分组

## Artifacts

- `./.yode/tools/*.md`
- `./.yode/tasks/*.log`
- `./.yode/memory/session.md`
- `./.yode/memory/session.live.md`
- `./.yode/transcripts/*.md`
