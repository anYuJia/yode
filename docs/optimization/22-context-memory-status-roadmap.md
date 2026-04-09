# Context / Memory / Hook / Diagnostics 阶段总结

## 范围

这份文档记录 Yode 在 2026-04-08 这一轮连续优化后，已经完成的上下文管理、session memory、hook 生命周期和诊断能力，并给出下一阶段的建议顺序。

持续任务跟踪见：`docs/optimization/23-100-step-optimization-tracker.md`

相关提交：

- `7282e3d` `feat(context): 引入分层指令加载与安全 include 解析`
- `dd8915c` `feat(context): 完善 compact 生命周期与熔断保护`
- `fe0f59b` `feat(session): 对齐 compact 持久化与手动 compact`
- `2b0b38e` `feat(hooks): 接通 session_start 生命周期`
- `8ff87ba` `feat(memory): 增强 live session memory 与 hook 结构化输出`
- `d0e385d` `feat(tui): 展示 compact 产物与来源`
- `b329ba7` `feat(diagnostics): 增强上下文状态与 memory 检视`
- `0905c4a` `feat(status): 显示最近 compact 与 memory 更新状态`
- `669d953` `feat(memory): 增强 transcript 元数据与浏览`
- `373d501` `feat(memory): 预览最新 transcript 摘要`
- `1090a18` `fix(memory): 同步异步 session memory 状态`
- `ec13e1b` `fix(memory): clarify compaction artifact expectations`

## 已完成

### 1. 分层指令加载

系统提示现在支持：

- `CLAUDE.md` / `CLAUDE.local.md`
- `YODE.md`
- `.claude/rules/*.md`
- 安全的 `@include`
- 循环保护
- 文本类型过滤
- 总字符上限

结果：系统提示不再只是平铺读一两个项目文档，而是具备 Claude Code 风格的层级规则加载。

### 2. compact 生命周期

已经接通：

- `pre_compact`
- `post_compact`
- 兼容旧事件 `context_compressed`

并且 compact 已统一走 engine 内部入口，不再在多处分散实现。

### 3. auto-compact 稳定性

已实现：

- 递归保护：`Compact` / `SessionMemory` query source 不会再次触发 auto-compact
- 熔断器：连续 3 次 compact 失败自动停机
- 手动 `/compact` 可以绕过 auto breaker

结果：长会话在异常情况下不会进入 compact 死循环。

### 4. compact 持久化

compact 之后现在会：

- 写 `./.yode/memory/session.md`
- 写 `./.yode/transcripts/*.md`
- 重写 session DB 消息快照

结果：`--resume` 不会再把 compact 前的整段历史重新灌回上下文。

另外，resume 之后现在会从磁盘重建 latest transcript / memory artifact 的运行时索引，`/status` 和 `/context` 不再回到全 `none`。

### 5. live session memory

已经有两层：

- `session.md`
  触发时机：compact
  作用：中长期保留 compact 摘要和 turn 级文件触达信息

- `session.live.md`
  触发时机：turn 完成后的阈值刷新 + session 退出 flush
  作用：中间层实时记忆

live memory 现在支持：

- 同步 snapshot 路径
- 异步 summarize 路径
- clear/reset 失效旧后台任务
- shutdown 最后一轮 flush

这一轮又补了：

- live/session memory 统一成稳定 section schema
- `Goals / Findings / Decisions / Files / Open Questions / Freshness / Confidence`
- 旧版非结构化 summary 会自动归一化到新 schema

### 6. hook 生命周期

已经打通的事件：

- `session_start`
- `session_end`
- `user_prompt_submit`
- `pre_turn`
- `pre_tool_use`
- `post_tool_use`
- `post_tool_use_failure`
- `permission_request`
- `permission_denied`
- `pre_compact`
- `post_compact`
- `context_compressed`

### 7. 结构化 hook 输出

`hooks.rs` 现在支持从 hook 的 stdout 解析最小 JSON 协议：

- `continue: false`
- `decision: "block"`
- `reason` / `stopReason`
- `modified_input` / `updatedInput`
- `systemMessage`
- `hookSpecificOutput.additionalContext`

最重要的是：`pre_tool_use` 的 `updatedInput` 已经真正贯穿到执行链，后续 validation、permission、dedup 和 tool execute 都会基于修改后的参数。

这一轮又补了：

- compact / session_end hooks 会收到 compact counters、breaker reason、live memory status
- hook 的 `hookSpecificOutput.memorySections` 可以直接返回结构化 memory markdown
- hook 的 `wakeNotification` / exit code `2` 会进入异步 wake 基础框架
- hook timeout / non-zero exit / execution error / wake 次数现在有聚合 telemetry

### 8. TUI / CLI 可观测性

现在用户可以直接看到：

- compact 是 `auto` 还是 `manual`
- compact 写出的 session memory 路径
- compact transcript 路径
- session memory 更新事件

新增/增强命令：

- `/status`
- `/context`
- `/memory`
- `/memory live`
- `/memory session`
- `/memory latest`
- `/memory list`
- `/memory list recent`
- `/memory list auto`
- `/memory list manual`
- `/memory list summary`
- `/memory list failed`
- `/memory list today`
- `/memory list yesterday`
- `/memory list <date-range>`
- `/memory compare <a> <b>`
- `/memory <index>`
- `/memory <filename>`

结果：context/memory 已经从“内部机制”变成“可直接诊断的用户界面能力”。

## 还缺什么

当前仍然缺的不是“有没有功能”，而是三个方向的深化：

### 1. memory 质量还偏轻

虽然 `session.live.md` 已有后台 summarize pass，但它仍然属于轻量级 session memory。

还没做的包括：

- 专门的 memory extraction query source
- 更强的 structured memory schema
- 对不同记忆类型的拆分（user/project/feedback/decisions）
- 从 compact 结果中自动抽稳定 facts

### 2. transcript 检视基础已收口

这一轮之后，transcript artifacts 的基础检视能力已经基本齐了：

- mode 过滤
- summary 过滤
- failed 过滤
- 组合过滤
- 日期范围过滤
- transcript 对比 + section summary + diff flags/preview
- transcript metadata: session memory path / file-touch summary
- fuzzy alias / latest compare shortcut
- `/sessions` 接入 latest transcript 摘要
- session history 与 transcript artifacts 通过持久化 metadata 稳定关联

下一步更值得投入的已经不是继续堆命令，而是提升 memory 本身的质量。

### 3. diagnostics 还停留在命令态

`/status` 和 `/context` 现在已经能看：

- compact 总次数 / auto 次数 / manual 次数
- 最近一次 breaker reason
- session memory update 次数
- failed tool result 计数

TUI turn status line 也已经补了 compact / live-memory 小指示。

但它还没有形成更完整的“上下文健康度仪表盘”。

例如还没做：

- compact 次数统计
- session memory update 次数统计
- 最近一次 breaker 打开原因
- prompt cache / compact 命中对比

## 建议的下一阶段顺序

建议严格按下面顺序推进，不要反过来。

### 阶段 A：Artifacts 检视增强

已完成：

1. `/memory list summary`
2. `/memory list failed`
3. `/memory list summary failed`
4. `/memory list <date-range>`
5. `/memory compare <a> <b>`

原因：

- 已有 artifact 产物
- 改动面主要在 TUI command 层
- 风险低
- 能显著提升“可调试性”

### 阶段 B：Structured Memory

优先做：

1. 新增 dedicated memory extraction pass
2. 把 `session.live.md` 从 markdown 自由文本升级成稳定 section schema
3. 区分 `Goals / Findings / Decisions / Files / Open Questions`

原因：

- 现在 memory 已经有基础持久层
- 下一步瓶颈是“质量”，不是“有没有”

### 阶段 C：Context Health Dashboard

已完成首批：

1. `/context` 增加 compact count / last breaker reason
2. `/status` 增加 session memory update 次数 / failed tool count
3. TUI 状态栏增加 compact/live-memory 小指示

原因：

- 这是对现有功能的压缩展示层
- 可以在不改底层架构的情况下快速提升体验

## 你接下来怎么做

如果你要继续让这条线走稳，建议你按下面顺序操作：

1. 先用现有命令验证这批能力是否符合预期。
   命令：
   - `/status`
   - `/context`
   - `/memory`
   - `/memory latest`
   - `/memory list auto`

2. 触发一次真实 compact，确认 artifacts 流程通。
   前提：
   - 当前 session 已经积累出可压缩内容
   - `/compact` 只有在实际移除消息或截断旧 tool result 时才会写 transcript
   预期：
   - TUI 出现 compact 提示
   - 能看到 `Session memory:` 路径
   - 能看到 `Transcript backup:` 路径
   - `/memory latest` 能直接查看最近 transcript 预览

   排障：
   - 如果提示 `Compaction made no changes`，说明当前上下文还不够长，或者虽然调用了 `/compact`，但没有发生实际压缩
   - 这时 `/memory latest` 和 `/memory list` 返回空是预期行为

3. 验证 hook 的 `updatedInput` 能否在你的实际 hook 配置里生效。
   重点测试：
   - `pre_tool_use` 改 path / command
   - permission hook 看见的是修改后的参数

4. 再决定下一阶段做哪条：
   - 如果你更关心“能查问题”，先做 artifacts 检视增强
   - 如果你更关心“长期上下文质量”，先做 structured memory

## 推荐下一刀

如果继续按照当前节奏推进，我建议下一刀做：

`structured live/session memory schema`

原因：

- artifacts 检视这一层已经够用
- 下一步瓶颈是 memory 质量，而不是“能不能看到 transcript”
- 继续做 structured memory 会比继续堆更多 `/memory list ...` 子命令更值
