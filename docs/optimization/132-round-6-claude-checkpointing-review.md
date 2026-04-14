# Round 6 Claude Checkpointing Review

## Scope

这份文档对应 round-6 tracker 的 `061`。基线参考为 2026 年 4 月 14 日可见的 Claude Code 官方文档：

- `https://code.claude.com/docs/en/checkpointing`
- `https://code.claude.com/docs/en/ide-integrations`

## Claude Baseline

- Claude Code 会在每次用户 prompt 和编辑前自动创建 checkpoint，并跨 session 保留。
- 它支持 `/rewind` 菜单里的 code restore、conversation restore、combined restore，以及 summarize from here。

## Yode Now

- Yode 已有手动 `/checkpoint save`、checkpoint inventory、branch snapshot、rewind anchor、restore dry-run、diff preview。
- 它也能把 checkpoint artifact 与 transcript、review、orchestration state 关联起来。

## Gap

- Yode 还没有 automatic per-prompt checkpoint，也没有真正 restore code/conversation 的 live primitive。
- Claude 的 checkpointing 已经是 default-on safety net；Yode 目前仍是 operator-triggered artifact workflow。
