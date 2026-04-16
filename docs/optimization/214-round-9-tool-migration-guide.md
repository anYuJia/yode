# Round 9 Tool Migration Guide

## Goal

把 round-8/早期 round-9 里主要依赖 slash command 包装的 runtime primitive，逐步迁移到 first-class tools 与 artifact-backed runtime。

## Migration Shape

- remote queue continuation: 从 `/remote-control run|complete|fail|ack` 过渡到 remote result ingestion / transport state
- sub-agent teamwork: 从单次 `agent` 调用过渡到 `team_create / send_message / team_monitor`
- hook defer: 从单次 hook failure 观察过渡到 defer state artifact + inspect family
- permission governance: 从“当前 prompt 是否确认”过渡到 layered settings + precedence chain

## Operator Rule

- slash command 继续保留，但应视为 operator surface，不再是唯一 runtime plane
- 任何新 primitive 都应补 runtime task、artifact、inspect alias、doctor surfacing 和 tests

## Migration Checklist

1. 先补 tool/runtime state。
2. 再补 artifact/backlink。
3. 再补 inspect/doctor/brief/status surface。
4. 最后补 quickstart、operator guide 和 verification。
