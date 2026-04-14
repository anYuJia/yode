# Round 7 Claude Branch Merge Review

## Scope

这份文档对应 round-7 tracker 的 `062`。基线参考：

- `https://code.claude.com/docs/en/checkpointing`
- `https://code.claude.com/docs/en/ide-integrations`

## Claude Baseline

- Claude 支持 fork conversation / rewind code / combined restore，并直接在 timeline 上操作。
- 它没有把 branch merge 显式暴露成 artifact，而是以内联 timeline actions 为主。

## Yode Now

- Yode 已有 branch snapshot、merge dry-run、merge preview artifact、restore conflict summary。

## Gap

- Yode 的 merge 仍是 artifact preview，没有执行层。
- Claude 的 control 更直接，而 Yode 仍依赖 operator bridge command。
