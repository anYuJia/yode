# Round 9 Remote Live State Quickstart

1. 运行 `/remote-control plan <goal>` 初始化 remote session、queue 和 live-session artifact。
2. 运行 `/remote-control transport connect` 建立 transport state。
3. 运行 `/remote-control monitor` 查看 live session inspector。
4. 运行 `/remote-control dispatch latest` 把最新 queue item 送入 remote runtime。
5. 运行 `/remote-control follow latest` 或 `/tasks follow latest` 持续跟进后台任务输出。
6. 运行 `/inspect artifact latest-remote-live-session-state` 和 `/inspect artifact latest-remote-transport-events` 复核状态与事件流。
