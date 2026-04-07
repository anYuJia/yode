# Yode 顶级 CLI 优化总方案（对标 Claude Code）

> 目标：把 Yode 从“可用的 Rust AI Agent CLI”升级为“顶级生产级 CLI”。
> 方法：基于 `yode` 现有源码现状 + `claude-code-rev` 可观测架构模式，做分层、可落地、可验收的系统优化。

---

## 1. 执行摘要（Executive Summary）

Yode 已具备非常好的基础：
- 核心引擎循环、工具调用、重试与超时、上下文压缩、成本追踪、权限系统、TUI 都已落地。
- Rust 架构清晰（`yode-core/yode-tools/yode-tui/yode-llm/yode-mcp/yode-agent`）。

但距离“顶级 CLI”仍有关键缺口，集中在：
1. **跨项目与 cwd 状态管理**（会话锚点/目录切换可靠性）
2. **工具失败恢复状态机**（避免连续失败后卡死/伪调用）
3. **权限与路径安全模型**（细粒度、解释性、安全边界）
4. **结果预算与大输出治理**（按消息批次治理，而非仅单次截断）
5. **任务化并发与可观测性**（后台任务、进度、诊断、SLO）

本方案给出 4 个阶段、12 条主线优化、可度量 KPI 与具体实施顺序。

---

## 2. 现状基线（基于 yode 源码）

### 2.1 已有优势

#### A. 引擎与循环能力完善
`crates/yode-core/src/engine.rs` 已具备：
- 多轮 tool-call loop
- streaming/non-streaming 两套路径
- LLM 重试分类（RateLimit/Transient/Fatal）
- 工具并行分发（只读且 auto-allow）
- tool result 截断（50KB）
- 成本跟踪与预算事件
- tool 调用预算注入与反思提示

#### B. 权限系统已有模式化能力
`crates/yode-core/src/permission.rs` 已有：
- 多模式：`Default/Plan/Auto/AcceptEdits/Bypass`
- 规则优先级（User/Project/Session/CLI）
- 命令风险分级（safe/risky/destructive）
- 拒绝追踪（denial tracker）

#### C. 工具类型系统清晰
`crates/yode-tools/src/tool.rs` 已有：
- 统一 `ToolContext`、`ToolCapabilities`、`ToolResult`
- typed error（validation/notfound/timeout/...）
- 可扩展 trait，利于持续加工具

#### D. 交互体验基础成熟
`crates/yode-tui/src/ui/chat.rs`：
- markdown 基础渲染
- tool call/result 样式与时长
- thinking/progress 展示
- CLI 风格接近现代 agent 终端

---

## 3. 对标 Claude Code 的关键模式（可借鉴）

结合 `claude-code-rev` 代码结构可见的高价值模式：

1. **会话级 cwd 覆盖**
   - `src/utils/cwd.ts`（异步上下文下 cwd override）
   - 价值：并发 agent 仍能目录隔离，不串 cwd

2. **跨项目恢复机制**
   - `src/utils/crossProjectResume.ts`
   - 价值：跨项目不是“盲跳”，而是显式 projectPath + 命令构建

3. **路径安全与权限决策细化**
   - `src/utils/permissions/pathValidation.ts`
   - 价值：glob、tilde、UNC、shell expansion、sandbox allowlist 全链路安全校验

4. **工具结果预算双层治理**
   - `src/constants/toolLimits.ts`
   - 价值：不仅限制单次结果，还限制“单消息内聚合结果”

5. **丰富任务模型与后台执行框架**
   - task/tool/services 体系完整
   - 价值：长任务不阻塞主对话，可监控可回收

---

## 4. 关键问题清单（严格版）

### P0（必须优先）

#### 4.1 失败恢复机制不够“状态机化”
虽然 `engine.rs` 有 `consecutive_failures` 和提示注入，但主要是“提示模型改变行为”，缺少**硬约束恢复流程**：
- 未形成统一 `on_error -> reanchor -> retry_with_strategy -> escalate` 状态机
- 容易出现“连续失败后中断或假性推进”

#### 4.2 cwd 只在上下文字段，不是完整会话状态机
当前主要依赖 `context.working_dir` 注入，缺少：
- 子任务/子 agent 的 cwd 继承与隔离机制
- 跨项目切换的原子流程（验证->切换->确认->落库）

#### 4.3 路径权限规则粗粒度
虽有命令分类与模式，但缺少更细粒度路径安全：
- shell expansion/UNC/tilde 变体等策略未系统化
- 对 glob/read/write/create 的不同策略深度不足

### P1（高价值）

#### 4.4 工具结果预算缺少“聚合上限”
当前主要是单工具结果截断（50KB），缺少：
- 同一轮多工具并行结果总量上限
- 超量后落盘 + preview 引导机制

#### 4.5 任务化能力不足
当前多以即时执行为主，缺少统一后台任务抽象（可暂停、查询、输出回放、终止）。

#### 4.6 可观测性不足
缺少系统化 SLO 指标与 tracing 事件规范，难以精确定位“卡住/中断/超时/权限阻断”的根因链路。

### P2（体验增强）

#### 4.7 大规模项目下的探索策略仍依赖模型自觉
虽有预算提示，但缺少“硬性流程模板”：
- 先锚定再搜索
- 失败降级策略
- 证据门禁（建议必须绑定证据）

#### 4.8 命令/工具协同约束需要进一步统一
当前对 bash 中 `find/grep` 有拦截是正确方向，但可继续收敛为“工具优先协议”。

---

## 5. 顶级 CLI 目标架构

```text
┌────────────────────────────────────────────────────┐
│                    Query Orchestrator              │
│  turn FSM + retry FSM + failure recovery FSM      │
└───────────────┬────────────────────────────────────┘
                │
┌───────────────▼────────────────────────────────────┐
│                 Session Runtime                     │
│ cwd state / project state / task state / budget    │
└───────┬───────────────────────┬────────────────────┘
        │                       │
┌───────▼─────────────┐ ┌───────▼────────────────────┐
│   Tool Execution     │ │ Permission & Path Engine   │
│ parallel/sequential  │ │ rule + classifier + path   │
│ timeout + budget     │ │ validation + sandbox        │
└───────┬─────────────┘ └───────┬────────────────────┘
        │                       │
┌───────▼───────────────────────▼────────────────────┐
│               Observability & Diagnostics           │
│ traces / events / error taxonomy / replay logs      │
└──────────────────────────────────────────────────────┘
```

---

## 6. 分阶段实施路线（可直接执行）

## 阶段 I（2~3 周）：稳定性与安全底座（最高优先）

### I-1. 引入“失败恢复状态机”
在 `engine.rs` 上新增独立恢复模块（建议 `recovery.rs`）：
- 错误分类：`path_not_found / permission_denied / timeout / validation / schema`
- 恢复动作：
  1) `reanchor_workspace`
  2) `switch_strategy`
  3) `single_step_retry`
  4) `ask_user_guidance`
- 规则：同类错误最多重试 1 次；第二次必须换策略

### I-2. 会话级 cwd 状态管理
新增 `SessionRuntime`：
- `session.cwd`
- `project_root`
- `active_project_id`
- `last_successful_tool_context`

所有工具调用强制携带 runtime cwd；子 agent 支持 cwd override。

### I-3. 路径验证引擎升级
对齐 Claude 模式，新增/强化：
- glob 基目录验证
- tilde 变体与 shell expansion 拦截
- 读写创建分离校验策略
- path traversal 与 symlink 语义一致化

---

## 阶段 II（2~4 周）：任务化与预算治理

### II-1. 任务框架（Task Runtime）
新增 `yode-tasks`（或先在 `yode-core` 内部模块化）：
- Task 类型：ShellTask / AgentTask / WorkflowTask
- 生命周期：create/list/get/stop/output
- 后台任务统一进度事件

### II-2. Tool result 双层预算
现有 50KB 单结果限制基础上新增：
- 单消息聚合预算（例如 200KB）
- 超预算自动持久化到临时文件
- 返回 preview + 文件引用给模型

### II-3. 输出可靠性协议
防“伪工具调用文本”：
- 协议层验证：assistant 文本中的 tool 标记必须可解析且可执行，否则降级为普通文本
- 工具调用必须来自结构化 metadata，不接受文本拼装

---

## 阶段 III（3~5 周）：顶级交互体验

### III-1. TUI 可观测层
- 工具调用时间线（start/progress/end）
- 当前 cwd/project/status 可视化
- 卡顿诊断提示（超时、权限阻断、重复调用）

### III-2. 大仓探索模式
引入 “Explore Protocol”：
1. root anchoring
2. minimal fact set
3. constrained search
4. evidence-gated conclusions

### III-3. 质量门禁
输出建议前必须具备：
- 至少 N 条证据（文件或命令输出）
- 建议与证据映射表

---

## 阶段 IV（持续）：生态与工程化

### IV-1. 命令系统深化
- `/doctor` 增强：权限、cwd、MCP、工具健康状态
- `/status` 增强：任务、预算、失败计数、重试策略
- `/sessions` 增强：跨项目恢复入口

### IV-2. SLO 与回归体系
- E2E 场景：跨目录、权限拒绝、连续失败、超大输出
- 回归指标自动化

---

## 7. 关键实现建议（结合当前 yode 代码）

### 7.1 `engine.rs` 解耦
当前 `engine.rs` 过重（LLM、工具、权限、恢复、注入都在一起）。建议拆分：
- `turn_executor.rs`：turn 主循环
- `tool_executor.rs`：并行/串行工具执行
- `retry_policy.rs`：LLM/tool 重试逻辑
- `failure_recovery.rs`：错误状态机
- `context_injection.rs`：智能注入策略

### 7.2 `PermissionManager` 增强建议
在 `permission.rs` 基础上增加：
- `DecisionReason`（可解释决策）
- `PathDecision`（路径级 allow/deny 原因）
- `AuditEvent`（决策日志）

### 7.3 `ToolContext` 增强建议
在 `tool.rs` 扩展：
- `session_runtime: Arc<...>`
- `trace_id/span_id`
- `project_scope`

便于跨目录、多任务下保持一致状态。

### 7.4 Result 存储策略
在 `ToolResult` 上支持：
- `is_truncated`
- `external_ref`（落盘路径/缓存键）
- `summary`（给模型的紧凑摘要）

---

## 8. KPI（验收标准）

## 8.1 稳定性
- 连续失败导致“会话中断率”下降 80%
- 工具调用成功率 > 97%
- 卡住（>30s 无有效进展）发生率下降 70%

## 8.2 跨项目能力
- 跨目录任务成功完成率 > 95%
- cwd 漂移问题（执行目录错误）< 0.5%

## 8.3 安全性
- 危险命令误放行率为 0
- 路径绕过（tilde/shell expansion/path traversal）回归测试全通过

## 8.4 用户体验
- 平均恢复时间（错误到继续执行）< 3 秒
- 大仓库分析首次有效结论时间下降 40%

---

## 9. 优先级待办（Top 12）

1. 引入失败恢复状态机（P0）
2. 建立会话级 cwd/runtime（P0）
3. 路径验证引擎升级（P0）
4. 工具调用协议校验防伪（P0）
5. 工具结果聚合预算与落盘（P1）
6. 任务化后台执行框架（P1）
7. `engine.rs` 职责解耦（P1）
8. 输出证据门禁（P1）
9. `/doctor` 与 `/status` 增强（P2）
10. 大仓探索流程模板化（P2）
11. 统一 tracing/audit 事件（P2）
12. E2E 回归场景矩阵（P2）

---

## 10. 风险与规避

1. **一次性重构过大**：分层渐进，先加 runtime 与 recovery，再迁移调用路径。
2. **性能回退**：为路径校验/预算治理加缓存，避免每次重复 IO。
3. **策略过严影响可用性**：保留模式开关（strict/normal/experimental）。
4. **行为漂移**：建立 golden transcript 回放测试，确保对话质量不退化。

---

## 11. 里程碑建议（8 周版本）

- **M1（第2周）**：恢复状态机 + cwd runtime 上线
- **M2（第4周）**：路径安全模型 + 协议校验上线
- **M3（第6周）**：任务框架 + 预算治理上线
- **M4（第8周）**：TUI诊断、SLO、E2E矩阵完成

---

## 12. 结论

Yode 已经具备成为顶级 CLI 的核心骨架。当前最关键不是“再加多少功能”，而是把**跨目录可靠性、失败恢复、权限路径安全、预算治理、任务化执行**这五个底层能力做到工业级。

当这五项到位后，Yode 将从“功能型 agent CLI”升级为“生产可依赖、可规模化使用的顶级 CLI”。
