# Yode Product Context

register: product

## 产品定位

Yode 是一个本地优先、Rust 原生的 AI 编程代理 runtime。现有产品以 CLI/TUI 为主，核心价值不是单轮聊天，而是在真实代码仓库中执行、观察、治理和恢复复杂开发任务。桌面端目标是把这些 runtime 能力转成更可见、更可控、更适合长会话的应用界面，同时保留 CLI/TUI。

## 目标用户

- 长时间在本地仓库中工作的开发者。
- 需要审查工具调用、文件修改、权限请求和上下文压缩状态的高级用户。
- 使用多个 LLM provider、MCP、hooks、子代理、workflow 和 remote task 的 operator 型用户。
- 希望获得接近现代桌面 coding agent 体验，但仍要求本地可审计和 Rust runtime 可复用的用户。

## 核心原则

- Runtime 可见：模型输出、工具调用、权限确认、成本、上下文压缩、任务恢复都应能被检查。
- 本地优先：配置、会话、artifact、checkpoint 和日志应保留在用户可审计的位置。
- 工程严谨：界面不制造“魔法感”，重点呈现状态、风险、下一步动作和可验证结果。
- 保守自动化：危险操作需要明确确认；只读和低风险动作可以更流畅。
- 中文优先：用户可见文案使用简体中文，provider、model、tool、MCP 等技术名词保留英文。

## 桌面端产品边界

桌面端不是 CLI 的图形包装层，也不应通过 shell 调用 Yode CLI 来完成核心能力。它应通过 Tauri commands/events 调用同一套 Rust crates，把 `yode-core`、`yode-tools`、`yode-llm`、`yode-mcp` 和 `yode-agent` 作为共享 agent runtime。

第一阶段桌面端只建立可运行的 mock shell：sidebar、topbar、timeline、composer、工具卡片、权限确认样式和设置 shell。真实 AgentEngine bridge 放到下一批。

## 反例

- 营销式首页、hero 大屏、装饰性插画，不适合这个产品。
- 把工具输出直接塞成一大段文本，不适合长任务。
- 复制 CLI/TUI 的所有信息密度，不适合桌面端。
- 使用不透明的自动执行，不展示权限、风险和可撤销性。
- 引入大型组件库导致视觉和交互被第三方默认风格主导。
