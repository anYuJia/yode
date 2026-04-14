# Round 7 Claude Cloud Restore Review

## Scope

这份文档对应 round-7 tracker 的 `065`。基线参考：

- `https://code.claude.com/docs/en/claude-code-on-the-web`

## Claude Baseline

- Claude web/cloud sessions可并行运行，并支持 teleport 回本地 terminal。
- session 可跨 surfaces 继续，而不仅是导出 artifact。

## Yode Now

- Yode 已有 remote-control, remote queue execution, handoff artifacts, checkpoint restore。

## Gap

- 仍无 cloud session transport
- 无 teleport-like remote session import/export primitive
