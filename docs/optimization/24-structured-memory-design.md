# Structured Memory Design

## Goal

将 `session.md` 与 `session.live.md` 统一成稳定的结构化 schema，便于：

- prompt 注入时做有选择的 memory 恢复
- `/memory` 命令稳定展示
- compaction / transcript / diagnostics 之间共享同一组字段

## Current schema

- `Goals`
- `Findings`
- `Decisions`
- `Open Questions`
- `Files Read`
- `Files Modified`
- `Freshness`
- `Confidence`

## Design notes

- schema 优先稳定字段名，不追求一次性覆盖所有 memory 类型
- 旧 markdown summary 允许降级解析，避免历史 session 丢失可读性
- compact 结果、live snapshot、hook memorySections 输出都映射到相同 section 集合

## Follow-up

- 可以继续拆成 `user/project/decision/feedback` 多层 memory
- 可以为每个 section 增加 source 与 confidence score
