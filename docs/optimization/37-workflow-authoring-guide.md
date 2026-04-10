# Workflow Authoring Guide

## Goal

为 `.yode/workflows/*.json` 提供一个稳定、可复用的编写约定，便于团队把常用检查流、审查流、交付流固化成脚本。

## 两种执行模式

- `workflow_run`
  只适合 safe/read-only workflow
  允许 `read_file`、`grep`、`glob`、`git_status`、`review_changes`、`verification_agent`、`coordinate_agents` 等读侧工具

- `workflow_run_with_writes`
  适合包含 `write_file`、`edit_file`、`git_commit`、`review_pipeline`、`review_then_commit` 这类写侧工具的 workflow
  使用前应先通过 `dry_run=true` 预览执行计划

## 推荐结构

```json
{
  "name": "ship-pipeline",
  "description": "Run review, verification, and commit when checks are clean",
  "steps": [
    {
      "tool_name": "review_pipeline",
      "params": {
        "focus": "${focus}",
        "commit_message": "${commit_message}",
        "test_command": "${test_command}"
      }
    }
  ]
}
```

## 编写原则

1. 先写 `dry_run` 友好的参数结构，再考虑真正执行。
2. 变量名用 `${focus}`、`${commit_message}` 这种显式语义字段，避免 `${x}` 这类短名。
3. 读写混合 workflow 应清楚区分哪一步会产生副作用。
4. 如果 workflow 会调用 `review_pipeline` / `review_then_commit`，建议在描述里写清 commit gate 条件。
5. 不要嵌套 workflow 调用；Yode 会阻止递归 workflow 执行。

## 建议流程

1. `/workflows init review-pipeline`
2. `/workflows show review-pipeline`
3. 用 `workflow_run` + `dry_run=true` 验证 plan
4. 确认 write-capable steps 后，再使用 `/workflows run-write <name>`

## 和 Claude Code 设计的对应关系

- `workflow_run` 对应 Claude Code 里默认保守的 workflow/tool orchestration
- `workflow_run_with_writes` 对应显式确认后的 mutation flow
- `write_steps` metadata 对应“执行前清楚告诉用户哪些步骤有副作用”的原则
