# Next 100 Optimization Tracker Round 9

## Scope

第九轮 100 项优化聚焦 `Yode` 当前工具调用体系与 `Claude Code` 官方能力之间的剩余差距，目标不是继续补零散命令，而是把“工具调用”这一层真正推进成产品级 runtime platform。

这一轮的主题：

- first-class tool surface
- permission/runtime governance parity
- hook protocol depth
- sub-agent / remote-control productization
- MCP / managed settings / operator UX

## Baseline

基线日期：`2026-04-14`

对照来源限定为 Claude Code 官方文档：

- [Claude Code Overview](https://docs.anthropic.com/en/docs/claude-code/overview)
- [Tools Reference](https://code.claude.com/docs/en/tools-reference)
- [Permission Modes](https://code.claude.com/docs/en/permission-modes)
- [Hooks](https://docs.anthropic.com/en/docs/claude-code/hooks)
- [Settings](https://docs.anthropic.com/en/docs/claude-code/settings)
- [Remote Control](https://code.claude.com/docs/en/remote-control)

当前 `Yode` 相关实现入口：

- [tool execution single-call runtime](/Users/pyu/code/yode/crates/yode-core/src/engine/tool_execution_runtime/single_call/mod.rs)
- [tool execution guards](/Users/pyu/code/yode/crates/yode-core/src/engine/tool_execution_runtime/single_call/guards.rs)
- [permission confirmation / deny flow](/Users/pyu/code/yode/crates/yode-core/src/engine/tool_execution_runtime/single_call/permissions.rs)
- [parallel tool-call partition and execution](/Users/pyu/code/yode/crates/yode-core/src/engine/streaming_turn_runtime/tool_calls.rs)
- [permission manager](/Users/pyu/code/yode/crates/yode-core/src/permission/manager/mod.rs)
- [permission explanation engine](/Users/pyu/code/yode/crates/yode-core/src/permission/manager/explain.rs)
- [hook event model](/Users/pyu/code/yode/crates/yode-core/src/hooks.rs)
- [tool registry / deferred tool pool](/Users/pyu/code/yode/crates/yode-tools/src/registry.rs)
- [sub-agent tool](/Users/pyu/code/yode/crates/yode-tools/src/builtin/agent/mod.rs)
- [remote control command surface](/Users/pyu/code/yode/crates/yode-tui/src/commands/dev/remote_control.rs)
- [remote control / transport artifacts](/Users/pyu/code/yode/crates/yode-tui/src/commands/dev/remote_control_workspace.rs)

## Gap Summary

相对 Claude Code，`Yode` 当前已经具备：

- 明确的 permission manager、rule source、deny tracker 与 bash guard
- `pre_tool_use / post_tool_use / permission_request / permission_denied` 等 hook 事件
- active/deferred tool pool 与 `tool_search`
- runtime task、artifact timeline、permission artifact、transport artifact 等可观测性
- 基础 sub-agent tool 与 remote-control / transport lifecycle 原语

但剩余差距仍然清晰：

- 很多能力仍停留在 slash command / workspace / artifact 层，而不是模型可直接调用的一等工具
- permission modes 还没有 Claude Code 那种 classifier + managed settings + enterprise policy 平面
- hook 协议深度不足，缺 `defer`、缺更完整的 subagent / worktree / task lifecycle 事件
- sub-agent 虽存在，但还没有 Claude Code 那种 team / message / background monitor 级产品面
- remote-control 仍偏 operator-driven runtime，不是 live multi-device session transport
- MCP 与 settings 还没有形成统一的 managed control plane

## Completion Standard

本轮“全部优化完成”定义为：

1. 关键能力以 first-class tools 形式出现，而不是主要依赖 slash command 包装。
2. permission / hooks / sub-agent / remote-control / MCP 至少达到 Claude Code 官方文档描述的核心产品面。
3. 所有新增能力都有 runtime artifact、inspect path、doctor surfacing、verification tests。
4. round-9 收口后，剩余差距应更多集中在产品取舍，而不是缺失的基础能力。

## Status

- `[ ]` 未开始
- `[~]` 进行中
- `[x]` 已完成

当前进度：

- `100 / 100` 已完成

## 001-010 Tool Surface Unification

- `[x]` 001 审计当前“slash command vs tool”能力分布
- `[x]` 002 形成 first-class tool candidate inventory
- `[x]` 003 把 remote queue dispatch 提升为 first-class tool
- `[x]` 004 把 remote queue complete/fail 提升为 first-class tool
- `[x]` 005 把 transport connect/reconnect/disconnect 提升为 first-class tool
- `[x]` 006 把 review / coordinate / workflow 中仍依赖命令桥接的核心路径改成 tool path
- `[x]` 007 建立 tool capability taxonomy（read/write/remote/background/team）
- `[x]` 008 建立 tool-to-command migration note
- `[x]` 009 为新一等工具补 runtime artifact backlinks
- `[x]` 010 tool surface review / closeout

## 011-020 Permission Modes And Governance

- `[x]` 011 对齐 Claude Code permission modes 名义与语义映射
- `[x]` 012 细化 `auto` 模式的风险级别与 fallback 路径
- `[x]` 013 为 `auto` 引入更独立的 command classifier 层
- `[x]` 014 支持 tool category 级规则，而不只按 tool name
- `[x]` 015 支持 managed / user / project / local 四层 settings 视图
- `[x]` 016 支持 managed permission rules artifact
- `[x]` 017 支持 allow/ask/deny 的 inspectable precedence chain
- `[x]` 018 补企业/托管 settings doctor surfacing
- `[x]` 019 permission governance verification tests
- `[x]` 020 permission parity review / closeout

## 021-030 Hook Protocol Parity

- `[x]` 021 明确 Yode hook protocol 与 Claude Code hooks 的事件映射
- `[x]` 022 增加 `subagent_start / subagent_stop` hook events
- `[x]` 023 增加 `task_created / task_completed` hook events
- `[x]` 024 增加 `worktree_create` hook event
- `[x]` 025 为 pre-tool hook 增加 `defer` 语义
- `[x]` 026 为 deferred tool call 增加 resume artifact / state
- `[x]` 027 为 hook result 增加 richer metadata snapshots
- `[x]` 028 为 hook wake/defer 增加 inspect family
- `[x]` 029 hook protocol verification tests
- `[x]` 030 hook parity review / closeout

## 031-040 Sub-Agent / Team Runtime

- `[x]` 031 审计现有 `agent` tool 与 Claude Code team/tool 面的差距
- `[x]` 032 增加 team creation/runtime artifact
- `[x]` 033 增加 send-message / handoff first-class tool
- `[x]` 034 增加 background sub-agent monitor surface
- `[x]` 035 增加 sub-agent lifecycle runtime tasks
- `[x]` 036 增加 sub-agent result bundle / inspect alias
- `[x]` 037 增加 parent-child permission inheritance strategy
- `[x]` 038 增加 sub-agent hook lifecycle surfacing
- `[x]` 039 sub-agent/team verification tests
- `[x]` 040 sub-agent/team parity review / closeout

## 041-050 Remote Control Live Session

- `[x]` 041 设计 true remote session state model
- `[x]` 042 将 transport artifact state 升级为 live session state
- `[x]` 043 remote queue complete/fail 从 operator command 升级为 remote result ingestion
- `[x]` 044 为 remote result ingestion 增加 resumable event log
- `[x]` 045 增加 reconnect session continuity model
- `[x]` 046 增加 remote session transcript sync
- `[x]` 047 增加 remote multi-endpoint identity / device model
- `[x]` 048 增加 remote-control live doctor surface
- `[x]` 049 remote live session verification tests
- `[x]` 050 remote-control parity review / closeout

## 051-060 MCP And Managed Settings

- `[x]` 051 对齐 Claude Code MCP / managed MCP 配置面
- `[x]` 052 增加 managed MCP inventory artifact
- `[x]` 053 增加 managed server policy / permission visibility
- `[x]` 054 将 MCP auth / transport / reconnect 统一接入 settings scope
- `[x]` 055 为 MCP 资源访问补 unified inspect family
- `[x]` 056 为 MCP failure / reconnect 补 operator remediation guide
- `[x]` 057 为 settings scopes 增加 doctor / inspect / export 路径
- `[x]` 058 settings / MCP control-plane verification tests
- `[x]` 059 MCP/settings parity review
- `[x]` 060 MCP/settings closeout

## 061-070 Runtime Observability

- `[x]` 061 为 tool call 增加统一 timeline phase model
- `[x]` 062 为 permission decisions 增加 richer runtime timeline merge
- `[x]` 063 为 deferred/defer hooks 增加 runtime timeline entries
- `[x]` 064 为 sub-agent/team runtime 增加 dedicated timeline entries
- `[x]` 065 为 remote live session 增加 timeline/state/artifact triad
- `[x]` 066 为 tool-search activation 增加 artifact / metrics
- `[x]` 067 为 managed settings decision chain 增加 diagnostic output
- `[x]` 068 为 tool failure clusters 增加 remediation summary
- `[x]` 069 observability verification tests
- `[x]` 070 observability closeout

## 071-080 Tool UX And Operator Surface

- `[x]` 071 统一 tool-facing 状态栏/turn status 术语
- `[x]` 072 统一 permission prompt 与 runtime badges
- `[x]` 073 为 tool / sub-agent / remote session 增加统一 inspect family
- `[x]` 074 统一 “dispatch / running / completed / failed / acked / deferred” 状态命名
- `[x]` 075 为 background work 增加 monitor / follow UX
- `[x]` 076 为 tool-search / hidden tools 增加更清晰的 operator affordance
- `[x]` 077 为 remote-control live state 增加 quickstart
- `[x]` 078 为 permission mode 选择增加 operator guide
- `[x]` 079 UX verification sweep
- `[x]` 080 UX closeout

## 081-090 Verification / Documentation / Migration

- `[x]` 081 为新一等工具补全 unit tests
- `[x]` 082 为关键工具链补 integration tests
- `[x]` 083 为 permission / hooks / remote-control 补 e2e scenario docs
- `[x]` 084 编写 slash-command -> first-class tool migration guide
- `[x]` 085 编写 managed settings rollout note
- `[x]` 086 编写 remote-control live session operator guide
- `[x]` 087 编写 sub-agent/team quickstart
- `[x]` 088 编写 hook defer quickstart
- `[x]` 089 verification / docs review
- `[x]` 090 verification / docs closeout

## 091-100 Final Parity Closeout

- `[x]` 091 重新对照 Claude Code Overview
- `[x]` 092 重新对照 Tools Reference
- `[x]` 093 重新对照 Permission Modes
- `[x]` 094 重新对照 Hooks
- `[x]` 095 重新对照 Settings
- `[x]` 096 重新对照 Remote Control
- `[x]` 097 round-9 gap map refresh after 50 items
- `[x]` 098 round-9 gap map refresh after 100 items
- `[x]` 099 round-9 final parity review
- `[x]` 100 round-9 release note draft

## Execution Order

建议执行顺序，不建议并行乱开：

1. `041-050` remote-control live session
2. `021-030` hook protocol parity
3. `011-020` permission modes and governance
4. `031-040` sub-agent / team runtime
5. `051-060` MCP and managed settings
6. `061-080` observability 与 UX 收口
7. `081-100` verification / docs / final parity closeout

## Notes

- round-8 已经把 remote transport、queue runtime binding、action/runtime observability 推到了“primitive 存在”的阶段；round-9 的目标不是继续堆 artifact，而是把这些 primitive 推进成真正的 tool platform。
- 如果后续执行过程中发现 Claude Code 官方文档基线发生变化，必须先刷新本文件的 `Baseline` 与对应 checklist，再继续实现。
