# Round 6 Claude Cloud Task Continuation Review

## Scope

这份文档对应 round-6 tracker 的 `064`。基线参考：

- `https://code.claude.com/docs/en/claude-code-on-the-web`

## Claude Baseline

- Claude Code on the web 支持 `--remote` 云端任务、parallel cloud sessions、`--teleport` 拉回本地、以及 `/tasks` 进入 cloud session continuation。
- 云端 session 可持续存在，并在 browser / mobile / terminal 间转移。

## Yode Now

- Yode 已有 remote task inventory、retry summary、follow prompt bridge、task handoff artifact。
- 这些能力开始把 remote task continuation 接到本地 runtime task model。

## Gap

- Yode 还没有真正 cloud session，也没有 teleport / pull-back / branch checkout 这类 continuation primitive。
- 目前 continuation 仍然是 artifact+prompt，而不是 runnable remote session。
