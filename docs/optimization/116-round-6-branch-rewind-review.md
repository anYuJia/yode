# Round 6 Branch And Rewind Review

## Scope

这份文档对应 round-6 tracker 的 `027`，总结 `021-026` 完成后 session branch / rewind 的状态。

当前 `yode`：

- `crates/yode-tui/src/commands/session/checkpoint.rs`
- `crates/yode-tui/src/commands/session/checkpoint_workspace.rs`
- `crates/yode-tui/src/commands/info/inspect.rs`
- `crates/yode-tui/src/commands/artifact_nav.rs`

## What Closed In 021-026

- checkpoint payload 已经扩成 branch / rewind 可复用的 snapshot model，而不是只面向普通 checkpoint。
- `/checkpoint branch ...` 能保存 branch snapshot、列 inventory、做 branch diff，并直接进 inspector。
- `/checkpoint rewind-anchor ...` 会写 transcript-backed rewind anchor，而 `/checkpoint rewind ...` 会生成 safety summary。
- inspect artifact aliases 已覆盖 latest branch / latest rewind anchor / state variants。

## Conclusion

- Yode 现在已经有 branch / rewind 的 artifact-level control surface。
- 真正剩下的缺口是把这些 preview 进一步推进成真实 restore / merge primitive，而不是继续只停留在 dry-run。
