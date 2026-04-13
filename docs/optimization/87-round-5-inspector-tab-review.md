# Round 5 Inspector Tab Review

## Scope

这份文档对应 round-5 tracker 的 `019`，用于总结 inspector tabs/data-source 这一批的状态。

当前 `yode`：

- `crates/yode-tui/src/commands/info/inspect.rs`
- `crates/yode-tui/src/commands/inspector_bridge.rs`
- `crates/yode-tui/src/ui/inspector.rs`

## What Closed In 011-018

- `/inspect tasks ...`
- `/inspect memory ...`
- `/inspect reviews ...`
- `/inspect status`
- `/inspect diagnostics`
- `/inspect doctor ...`
- `/inspect hooks`
- `/inspect permissions ...`

这些输出都会通过 shared bridge 自动拆成 inspector tabs，不再只是单页长文本。

## Conclusion

- 虽然还不是 fully interactive pane system，但 data-source 层已经真的接上了 inspector runtime。
- 下一步更值得做的是 inspector search/jump/focus depth，而不是继续加更多静态 tab。
