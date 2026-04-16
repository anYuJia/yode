# Round 9 Remote Live Session Operator Guide

## Core Surfaces

- `/remote-control latest`
- `/remote-control monitor`
- `/remote-control queue`
- `/remote-control follow latest`
- `/inspect artifact latest-remote-live-session-state`
- `/inspect artifact latest-remote-transport-events`

## Normal Flow

1. `plan`
2. `transport connect`
3. `dispatch`
4. `follow`
5. `complete/fail/ingest`
6. `doctor` 与 `bundle`

## Recovery Flow

- transport 断开时先跑 `/remote-control transport reconnect`
- queue 有 `needs-attention` 时先看 execution artifact 和 transport events
- live session 状态漂移时先跑 `/remote-control session sync`
- 需要移交时写 `/remote-control handoff latest`

## Monitor Rule

- `monitor` 看 session 面
- `tasks monitor` 看后台任务面
- `follow latest` 看单个任务输出
