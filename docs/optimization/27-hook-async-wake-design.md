# Hook Async Wake Design

## Goal

允许 hook 在主 turn 之外发出异步唤醒信号，让 UI 能看到“后台事件完成”。

## Protocol

hook stdout 可返回：

- `hookSpecificOutput.wakeNotification`

非零退出码但包含 wake payload 时：

- 不当成致命错误
- 进入 wake queue

## Runtime exposure

- hook wake count 聚合到 runtime state
- TUI 将 wake 作为 system message 注入
- `/hooks` / `/status` / `/diagnostics` 都可见

## Extension

- 同一机制可复用到 background task notifications
