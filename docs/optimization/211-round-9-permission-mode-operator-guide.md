# Round 9 Permission Mode Operator Guide

## Core Modes

- `default`: 风险工具需要确认，适合日常开发。
- `plan`: 阻断写入与高风险执行，适合 review / inspect / planning。
- `auto`: classifier 自动放行低风险动作，风险升级时回退到 ask。
- `accept-edits`: 自动放行编辑类工具，但 shell 风险仍保留更强 guardrail。
- `bypass`: 跳过确认，只适合短时、受控、本地可信流程。

## TUI Note

- `Shift+Tab` 的 TUI badge cycle 只覆盖 `default -> auto -> plan`。
- `accept-edits` 和 `bypass` 需要显式运行 `/permissions mode <mode>`。

## Recommended Flow

1. 先用 `/permissions mode` 读取当前 mode 与 operator guide。
2. 风险不明确时保持 `default`。
3. 只读分析切到 `plan`。
4. 读多写少、希望更快时切到 `auto`。
5. 明确知道要密集改文件时再切 `accept-edits`。
6. 只有在本地可信环境、且会主动复核结果时才切 `bypass`。

## Supporting Commands

- `/permissions governance`
- `/permissions explain bash`
- `/inspect artifact latest-permission-governance`
- `/status`
