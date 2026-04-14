# Round 6 Direct Action Review

## Scope

这份文档对应 round-6 tracker 的 `057`，总结 `051-056` 完成后 inspector direct action 的状态。

当前 `yode`：

- `crates/yode-tui/src/ui/inspector.rs`
- `crates/yode-tui/src/commands/artifact_nav.rs`
- `crates/yode-tui/src/commands/info/inspect.rs`
- `crates/yode-tui/src/commands/tools/workflows/actions.rs`
- `crates/yode-tui/src/commands/dev/coordinate.rs`
- `crates/yode-tui/src/commands/session/checkpoint.rs`

## What Closed In 051-056

- inspector 现在有显式 action descriptor model，而不是只有 footer 文本。
- artifact inspect 会把 refresh / rerun / follow-up command 作为 action row 暴露出来。
- workflow、coordinate、checkpoint、remote-control 都已经把各自的 rerun / diff / bundle / doctor command bridge 成 direct actions。

## Conclusion

- Yode 已经开始从“看到命令字符串”迈向“在 inspector 里看到可执行动作”。
- 真正剩下的仍是 action dispatch 本身，而不是 action 语义的缺失。
