# Yode Product Context

register: product

## 产品定位

Yode 是一个 **Desktop GUI-first、本地优先、Rust 原生的 AI 编程 Agent**。正式产品入口是 `apps/yode-desktop/`（Tauri + React），核心 Agent 能力由 Rust runtime crates 提供，再通过 Tauri commands/events 暴露给 GUI。

Yode 已停止 CLI/TUI 产品路线。仓库根目录是 virtual Cargo workspace，不再提供根 `yode` binary、TUI、`clap` 命令树、shell completion 或 Cargo-install 产品路径。内部 Agent 仍可以使用 bash / PowerShell / terminal 类工具执行开发任务，但这些属于 Agent runtime capability，不是用户产品入口。

Yode 的核心价值不是单轮聊天，而是在真实代码仓库中完成可持续的长任务闭环：理解任务 → 建立验收标准 → 规划 → 执行 → 验证 → 失败重规划 → 交付 → 评测与学习。

## 目标用户

- 希望在桌面工作区内把 AI Agent 当作长期开发协作者的开发者。
- 需要审查工具调用、文件修改、权限请求、验证证据、成本和运行状态的高级用户。
- 使用多个 LLM provider、MCP、hooks、子 Agent、workflow、worktree、browser 和 remote runtime 的用户。
- 要求代码执行过程可审计、失败可恢复、完成结果可验证的团队和个人开发者。

## 核心原则

- **GUI-first**：用户能力首先设计成 Desktop interaction，而不是新增 CLI command。
- **Runtime 可见**：模型输出、工具调用、权限、成本、context、任务图、验证、恢复状态都应能在 GUI 中检查。
- **验证后交付**：修改代码的 run 不应只凭模型文本宣布成功，需要 Acceptance Criteria 与 Evidence 支撑。
- **本地优先**：配置、会话、artifact、index、evaluation、checkpoint 和 recovery 信息应保留在用户可审计的位置。
- **Fail closed**：浏览器、权限、截断流、验证、sandbox 等边界宁可明确失败，也不伪造成功。
- **少堆工具，多做闭环**：优先提升 planning、retrieval、parallel execution、verification、replanning、learning，而不是无限增加工具数量。
- **中文优先**：用户可见文案使用简体中文，provider、model、tool、MCP、runtime 等技术名词保留英文。

## 产品架构边界

```text
Desktop React UI
      ↓
Tauri commands / events
      ↓
yode-core / yode-agent / yode-tools / yode-runtime
      ↓
yode-llm / yode-mcp / yode-index / yode-evals
```

- React 负责交互和呈现，不承载核心 Agent 决策逻辑。
- Tauri 后端负责 Desktop adapter、生命周期和 IPC，不复制核心 engine 能力。
- 可复用 Agent 能力应优先进入 Rust crates，以便测试、恢复、评测和后台执行。
- 不通过 shell 启动已删除的 Yode CLI 来完成 Desktop 核心功能。

## 当前能力建设优先级

### P0 — 已进入主线

1. YodeBench / `yode-evals`
2. RunController + 状态机
3. Acceptance Criteria + Verification Gate
4. Repository Intelligence / `yode-index`
5. Parallel DAG Scheduler
6. Real Browser Runtime

### P1 — 当前重点

1. Model Capability Registry + ModelRouter
2. OS Sandbox
3. Agent worktree 自动分配与 merge
4. Tool semantic routing
5. GitHub issue → PR → CI 闭环
6. Postmortem + Learning

### P2

1. Cloud / SSH / Docker ExecutionBackend
2. Best-of-N / Judge / Debate
3. Multi-repo Agent
4. 更高级的 Agent UI / RunInspector

## Desktop UX 目标

Desktop 不是 CLI 的图形包装层，而是 Agent control plane：

- 主工作区围绕 conversation / run timeline，而不是命令输出终端。
- RunInspector 展示计划、并行节点、工具 trace、验证证据、成本、失败与恢复。
- 权限请求、风险操作、browser/computer-use、worktree/merge 必须有清晰的 GUI 状态和反馈。
- 长任务允许用户理解“Agent 正在做什么、为什么做、是否通过验证、失败后如何恢复”。
- 设置页面负责 provider/model、MCP、browser、hooks、permissions、sandbox 等正式配置入口。

## 反例

- 重新加入根 `src/main.rs`、TUI 或 `clap` 命令树。
- 为新能力先做 CLI command，再把 GUI 当二等入口。
- 在 Desktop 后端通过调用 Yode CLI 完成核心逻辑。
- 模型没有真实执行或验证，却在 GUI 显示成功。
- 工具很多，但缺少 task graph、acceptance、verification、replan 和 delivery 闭环。
- 把所有 runtime 信息堆成大段日志，而不是结构化可检查状态。
- 使用不透明自动执行，不展示权限、风险、证据和可撤销性。
