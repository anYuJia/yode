# Round 6 Claude Rewind And Branch Review

## Scope

这份文档对应 round-6 tracker 的 `062`。基线参考：

- `https://code.claude.com/docs/en/checkpointing`
- `https://code.claude.com/docs/en/ide-integrations`

## Claude Baseline

- Claude Code 在 VS Code 里支持 Fork conversation from here、Rewind code to here、Fork conversation and rewind code。
- 这些动作直接绑定到 conversation timeline，而不是离线 artifact 文件。

## Yode Now

- Yode 已有 `/checkpoint branch save`、branch inventory、branch diff、rewind anchor、rewind safety summary。
- inspect / artifact inventory / brief / status 已能把 branch 和 rewind 产物串起来。

## Gap

- Yode 的 branch / rewind 仍是 snapshot artifact，而不是 live session timeline action。
- Claude 的 fork/rewind 在 IDE 中是 inline action；Yode 仍通过 command bridge 和 inspector action row 间接完成。
