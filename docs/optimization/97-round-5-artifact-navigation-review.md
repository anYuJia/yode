# Round 5 Artifact Navigation Review

## Scope

这份文档对应 round-5 tracker 的 `056`，总结 `051-055` 落地后的 artifact navigation 状态。

当前 `yode`：

- `crates/yode-tui/src/commands/artifact_nav.rs`
- `crates/yode-tui/src/commands/info/inspect.rs`
- `crates/yode-tui/src/commands/utility/export.rs`
- `crates/yode-tui/src/commands/utility/export/shared.rs`
- `crates/yode-tui/src/commands/info/status.rs`

## What Closed In 051-055

- workspace index 现在带 orchestration artifact 和 inspect alias，而不是只列 conversation/runtime 文件。
- `/inspect artifact ...` 可以直接打开 status、remote、bundle 产物。
- bundle workspace index 已经能作为一个固定 manifest 被读取。
- artifact freshness badge 和 stale refresh action 已进入 inspector。

## Conclusion

- artifact navigation 这条线已经从“知道文件在哪”提升到“知道该看哪个 artifact、何时该刷新它”。
- 剩下的价值更偏向 direct actions 和 richer alias resolution，而不是再加更多路径字符串。
