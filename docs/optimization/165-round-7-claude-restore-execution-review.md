# Round 7 Claude Restore Execution Review

## Scope

这份文档对应 round-7 tracker 的 `061`。基线参考：

- `https://code.claude.com/docs/en/checkpointing`
- `https://code.claude.com/docs/en/ide-integrations`

## Claude Baseline

- Claude Code 的 restore 可直接恢复 code、conversation 或两者，并内建在 rewind menu / hover action 中。
- checkpoint 在每次 prompt / edit 前自动创建，并跨 session 保留。

## Yode Now

- Yode 已有真实 `/checkpoint restore <target>`，会替换 live engine/db snapshot。
- restore 之前还有 dry-run、rewind safety summary、restore doctor。

## Gap

- Yode 仍缺 automatic checkpointing。
- Claude 的 restore 是 default flow；Yode 仍是 operator-directed workflow。
