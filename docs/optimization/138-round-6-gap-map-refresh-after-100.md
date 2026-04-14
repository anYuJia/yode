# Round 6 Gap Map Refresh After 100

## What Round-6 Closed

- orchestration runtime 从 TUI shell 推进到 tool-side state
- session checkpoint / branch / rewind 进入 artifact-level reversible control surface
- remote control / remote task continuation 进入 command+artifact workspace
- inspector direct actions 从文案提示升级为模型和渲染路径

## Remaining Gaps

- 真正的 restore / branch merge / rewind execution 还没接进 engine
- remote control 仍非 live session transport
- action dispatch 仍停在 visible bridge，不是 direct execution

## Direction

下一轮应优先把 restore、remote queue execution、action dispatch 三件事推进成 runtime primitive。
