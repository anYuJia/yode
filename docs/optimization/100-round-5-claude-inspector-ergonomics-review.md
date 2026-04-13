# Round 5 Claude Inspector Ergonomics Review

## Scope

这份文档对应 round-5 tracker 的 `061`。基线参考的是 2026 年 4 月 13 日可见的 Claude Code 官方文档：

- `https://code.claude.com/docs/en/ide-integrations`
- `https://code.claude.com/docs/en/commands`

## Claude Baseline

- Claude Code 在 VS Code 里已经有原生图形面板、side-by-side diff review、conversation history 搜索、remote/local session picker，以及多会话 tab 并行入口。
- 它还能从外部工具直接打开新的 Claude tab，并共享 selection、terminal output、IDE diagnostics。

## Yode Now

- `yode-tui` 已经有 interactive inspector runtime、tab cycle、search、focus badge、stack/handoff。
- 新一轮补上了 workflow/coordinator/artifact inspector，所以可见性已经不再是主要短板。

## Gap

- Yode 仍是 terminal-first inspector，没有 IDE-native diff review、conversation tab persistence、URI-based handoff。
- Inspector 里的 action 仍是 command string，不是直接的 clickable or embedded action model。

## Conclusion

- round-5 让 Yode 在 terminal inspector 这条线上明显接近 Claude Code CLI。
- 真的差距已经从“有没有 inspector”变成“inspector 是否跨终端/IDE/remote surface 连续可用”。
