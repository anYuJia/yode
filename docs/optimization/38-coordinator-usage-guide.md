# Coordinator Usage Guide

## Goal

说明 `coordinate_agents` 在 Yode 里的推荐用法，以及它目前和 Claude Code coordinator 的差距。

## 何时使用

- 同一个目标可以被拆成 2-4 个独立 workstream
- 需要先 review 再 verify 这种依赖链
- 需要并行探索多个方案，再统一收敛

## 核心参数

- `goal`
  总目标

- `workstreams`
  每个 workstream 包含：
  - `id`
  - `description`
  - `prompt`
  - `depends_on`
  - `allowed_tools`
  - `run_in_background`

- `dry_run`
  只输出 phase/batch plan，不真正启动 sub-agent

- `max_parallel`
  限制同一 phase 内最多并发多少个 workstream

## 示例

```json
{
  "goal": "ship the feature safely",
  "dry_run": true,
  "max_parallel": 2,
  "workstreams": [
    {
      "id": "review",
      "description": "review changes",
      "prompt": "review the current workspace changes and report findings first"
    },
    {
      "id": "verify",
      "description": "verify behavior",
      "prompt": "verify the implementation and highlight regressions or missing tests",
      "depends_on": ["review"]
    }
  ]
}
```

## 推荐实践

1. 先用 `dry_run=true` 看 phase 和 batch。
2. 如果 phase 里 ready workstreams 太多，设置 `max_parallel`。
3. 每个 workstream 的 prompt 应尽量聚焦，不要让多个 sub-agent 做完全同样的事。
4. 依赖链里后置 workstream 应引用前置 workstream 的输出，而不是重新从零探索。

## 当前差距

- 还没有更复杂的 dynamic rescheduling
- 还没有 workstream 级别的 partial retry
- 还没有专门的 coordinator timeline UI
