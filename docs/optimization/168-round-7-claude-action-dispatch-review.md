# Round 7 Claude Action Dispatch Review

## Scope

这份文档对应 round-7 tracker 的 `064`。基线参考：

- `https://code.claude.com/docs/en/ide-integrations`

## Claude Baseline

- Claude Code 在 IDE 里直接提供 rewind/fork/diff/open-in-terminal/open-in-new-tab 等原生 actions。

## Yode Now

- Yode inspector 已支持 action selection、focus、`Ctrl+Enter` dispatch、action safety summary、action history artifact。

## Gap

- Claude 的 action feedback 更产品化，Yode 仍主要走 command output surfaces。
- Yode 还缺 richer modal / per-action post-run UI。
