# Task Runtime Troubleshooting Guide

## 目录

- `.yode/tasks/`
  后台 bash / agent 的输出日志

- `.yode/agent-results/`
  同步 sub-agent 的结果 artifact

## 常见问题

### 1. `/tasks` 能看到任务，但没有输出

检查：

1. `/tasks <id>`
2. `/tasks read <id>`
3. `task_output` 是否报 `output_path` 读取失败

### 2. 任务一直 running

检查：

1. `/tasks latest`
2. 看 `Progress`、`Progress at`
3. 看 `Recent progress`

如果是 agent 类任务，优先用 `task_output follow=true` 等待它结束，而不是手动轮询。

### 3. agent 输出太长，看不到开头

`task_output` 对 agent 输出默认采用“头部 + 尾部折叠”，保留：

- 开头的目标/结论
- 尾部的最新执行结果

中间会显示：

```text
... [agent output folded: N middle lines omitted] ...
```

### 4. 重试后 task 很乱

现在 task 有：

- `attempt`
- `retry_of`

可以用 `/tasks latest failed` 找最近失败任务，再看后续重试链。

### 5. 任务通知太吵

当前通知已分级：

- `success`
- `warning`
- `error`

后续可以继续做更细的 TUI 分组/折叠。
