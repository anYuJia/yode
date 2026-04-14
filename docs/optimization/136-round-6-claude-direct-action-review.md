# Round 6 Claude Direct Action Review

## Scope

这份文档对应 round-6 tracker 的 `065`。基线参考：

- `https://code.claude.com/docs/en/ide-integrations`

## Claude Baseline

- Claude Code 在 VS Code 里直接提供 diff accept/reject、Open in New Tab、Open in Terminal、fork/rewind actions。
- CLI 连接 IDE 时也能打开 VS Code 原生 diff viewer，而不是只打印命令提示。

## Yode Now

- Yode inspector 已有 action row model，并把 artifact refresh、workflow rerun、coordinate rerun、checkpoint diff/restore、remote-control doctor/bundle 暴露为 direct actions。
- 这让 Yode 不再只有 footer command 字符串。

## Gap

- Yode 的 direct actions 仍只是显示 command bridge，并未做到一键 dispatch。
- Claude 的 action 已是 product-native interaction；Yode 目前只完成 action semantics 和 render path。
