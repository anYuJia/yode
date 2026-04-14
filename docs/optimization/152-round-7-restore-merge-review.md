# Round 7 Restore And Merge Review

## Scope

这份文档对应 round-7 tracker 的 `037`，总结 `031-036` 完成后 restore / branch merge 的状态。

当前 `yode`：

- `crates/yode-tui/src/commands/session/checkpoint.rs`
- `crates/yode-tui/src/commands/session/checkpoint_workspace.rs`
- `crates/yode-tui/src/commands/info/doctor/mod.rs`

## What Closed

- branch merge preview 已有独立 preview model 和 artifact。
- `/checkpoint branch merge-dry-run <target>` 已经能实际生成 merge preview，而不是只输出文本 diff。
- restore doctor 已经把 checkpoint/branch/rewind/merge preview 这几类 artifact 统一检查进来。

## Conclusion

- Yode 已具备 restore / merge 的 control-plane preview。
- 真正剩下的是 merge execution 本身，而不是 preview / doctor / inspect 可见性。
