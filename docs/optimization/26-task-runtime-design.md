# Task Runtime Design

## Goal

为后台 bash / background agent 提供统一 task runtime：

- 统一数据模型
- 统一输出文件
- 可取消
- 可通知
- `/tasks` 可检查

## Data model

`RuntimeTask` 字段：

- `id`
- `kind`
- `source_tool`
- `description`
- `status`
- `output_path`
- `created_at`
- `started_at`
- `completed_at`
- `last_progress`
- `error`

## Lifecycle

1. tool 创建 task entry
2. task 进入 `running`
3. 输出写入 `./.yode/tasks/*.log`
4. 完成后进入 `completed/failed/cancelled`
5. 生成 notification

## Retention

- 保留最近 20 个已完成 task
- 超出后清理最老的完成态 task 及其输出文件
