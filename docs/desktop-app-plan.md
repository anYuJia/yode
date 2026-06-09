# Yode Desktop App 迁移计划

目标：把 Yode 从终端原生 AI 编程助手扩展为桌面端应用，同时保留现有 CLI/TUI 能力。桌面端使用 Tauri v2 作为 Rust 原生桌面壳，前端使用现代 TypeScript 技术栈，UI 参考当前 Codex 桌面端的信息架构和交互质感。

本计划是第 0 批交付物：冻结方向、架构、信息架构、事件协议草案、分批任务和 MVP 验收标准。本文不包含最终视觉规范；仓库当前没有 `PRODUCT.md` / `DESIGN.md`，正式开 UI 前需要补充产品与设计上下文。

## 1. 结论

Yode 不应通过“前端套 CLI 命令”的方式桌面化。正确路线是把现有 Rust crates 作为共享 agent runtime，新增一个 Tauri 桌面客户端：

- CLI/TUI 继续存在。
- Desktop 通过 Tauri commands/events 调用同一套 `yode-core`、`yode-tools`、`yode-llm`、`yode-mcp`、`yode-agent`。
- 前端只负责交互、渲染、状态管理和用户确认。
- Rust 后端负责会话、模型、权限、工具执行、数据库、MCP、自动化和流式事件。

这样可以避免重复实现 agent 逻辑，也能为后续 Web/远程客户端留下统一协议。

## 2. 技术栈

建议栈：

- Desktop shell：Tauri v2
- Frontend：React + TypeScript + Vite
- State：Zustand
- Server state：TanStack Query，仅用于设置、会话列表、项目列表等请求式数据
- Styling：Tailwind CSS + CSS variables
- Icons：lucide-react
- Code/diff：CodeMirror 6 起步，后续需要更重编辑器时再引入 Monaco
- Virtualization：@tanstack/react-virtual
- Rust bridge：Tauri `#[tauri::command]` + `AppHandle::emit`
- Persistence：沿用 `yode-core::db::Database` 和 `.yode` artifacts

不建议第一版引入 Electron。Yode 核心是 Rust，Tauri 可以直接复用现有 crates，打包体积、权限模型和原生集成更符合项目方向。

## 3. 工程目录

推荐目录：

```text
yode/
├── apps/
│   └── yode-desktop/
│       ├── package.json
│       ├── vite.config.ts
│       ├── src/
│       │   ├── app/
│       │   ├── components/
│       │   ├── features/
│       │   ├── lib/
│       │   ├── stores/
│       │   └── styles/
│       └── src-tauri/
│           ├── Cargo.toml
│           ├── tauri.conf.json
│           └── src/
│               ├── main.rs
│               ├── commands.rs
│               ├── events.rs
│               ├── runtime.rs
│               └── state.rs
├── crates/
│   ├── yode-core/
│   ├── yode-tools/
│   ├── yode-llm/
│   ├── yode-mcp/
│   └── yode-agent/
└── docs/
```

`apps/yode-desktop/src-tauri` 作为新的 Rust binary crate，依赖 workspace 内现有 crates。前端代码不直接访问 Yode 业务文件，只通过 Tauri command/event。

## 4. 架构拆分

### 4.1 后端层

新增 `DesktopRuntime`，职责：

- 管理多个桌面会话。
- 创建和恢复 `AgentEngine`。
- 持有 active turn 的 cancellation token。
- 接收前端发送的用户消息。
- 将 `EngineEvent` 转换为桌面端稳定事件。
- 处理工具确认、AskUser 回复、取消、重试。
- 暴露设置、provider、MCP、权限、项目列表和会话列表。

现有 `AgentEngine` 已经有适合桌面桥接的事件枚举，见 `crates/yode-core/src/engine/types.rs`：

- `TextDelta`
- `ReasoningDelta`
- `ToolCallStart`
- `ToolConfirmRequired`
- `ToolProgress`
- `ToolResult`
- `ContextCompactionStarted`
- `ContextCompressed`
- `CostUpdate`
- `SuggestionReady`
- `SessionMemoryUpdated`
- `Done`

第一阶段不应重写这套事件。应新增一个 DTO 层，把 Rust 内部类型转成可序列化的桌面事件。

### 4.2 前端层

前端分为四类状态：

- App chrome：当前页面、sidebar 折叠、设置页 tab、主题。
- Session state：会话列表、当前会话、项目列表。
- Turn state：流式文本、工具调用、权限请求、错误、取消状态。
- UI state：展开/折叠、选中的 artifact、diff panel、scroll anchor。

前端必须把 timeline 视为 append-only event log，而不是简单字符串拼接。工具卡片、文件卡片、压缩边界、成本更新、子代理结果都应是结构化节点。

### 4.3 协议层

定义稳定的桌面协议，不直接把内部 Rust enum 暴露给前端。原因：

- 内部 engine event 会继续演进。
- 前端需要 UI 专用字段，例如 display title、severity、collapsible、artifact path。
- 桌面端需要兼容会话恢复和持久化。

建议新增 DTO：

```rust
#[derive(serde::Serialize)]
pub struct DesktopEvent {
    pub session_id: String,
    pub turn_id: String,
    pub seq: u64,
    pub kind: DesktopEventKind,
    pub timestamp: String,
}
```

`DesktopEventKind` 第一版：

```text
turn_started
assistant_text_delta
assistant_reasoning_delta
assistant_text_complete
tool_started
tool_progress
tool_result
tool_confirmation_required
tool_confirmation_resolved
ask_user_required
context_compaction_started
context_compacted
cost_updated
suggestion_ready
session_memory_updated
turn_completed
turn_failed
turn_cancelled
runtime_state_updated
```

## 5. Tauri Commands 草案

第一版 commands：

```text
app_get_bootstrap()
settings_get()
settings_update(patch)
providers_list()
providers_set_default(name)
projects_list()
sessions_list(project_root?)
sessions_create(project_root, provider?, model?)
sessions_resume(session_id)
sessions_rename(session_id, title)
sessions_archive(session_id)
turn_send_message(session_id, content, attachments?)
turn_cancel(session_id, turn_id)
tool_confirm(session_id, turn_id, tool_call_id, decision)
ask_user_answer(session_id, question_id, answer)
runtime_state_get(session_id)
artifacts_open(path)
files_open(path, line?)
```

第一版不要实现所有设置项。只需要保证架构能承载它们。

## 6. UI 信息架构

参考 Codex 桌面端，但 Yode 不应完全复制视觉。第一版信息架构：

### 6.1 主窗口

左侧 sidebar：

- 新对话
- 搜索
- 技能
- 插件
- 自动化
- 项目分组
- 最近对话
- 设置

顶部栏：

- 当前会话标题
- 工作区路径
- 模型 / provider
- 权限模式
- 运行状态
- 更多菜单

主区域：

- timeline
- 工具卡片
- 文件卡片
- diff 卡片
- plan 卡片
- compact boundary
- 子代理 / task notification

底部 composer：

- 多行输入
- 附件按钮
- 权限模式选择
- provider/model/effort
- 发送 / 停止

右侧 inspector，可选：

- 当前工具详情
- diff
- artifact
- runtime state
- cost/context
- logs

### 6.2 设置页

第一版设置页结构：

- 常规
- 外观
- 配置
- 个性化
- 键盘快捷键
- MCP 服务器
- 钩子
- 连接
- Git
- 环境
- 工作树
- 浏览器
- 电脑操控
- 已归档对话

设置页第一批只做 shell 和常规页，其他 tab 可显示占位空态。

## 7. 视觉原则

当前只有截图参考，没有产品设计文档。先采用保守原则：

- 深色工作台优先，适合长时间代码任务。
- 左侧栏使用低对比毛玻璃/磨砂感，但不要滥用 blur。
- 主内容区保持低噪声，强调 timeline 层级。
- accent 使用 Yode 品牌色，初始可沿用 Codex 风格的粉色，但后续应定 Yode 自己的 token。
- 卡片只用于工具调用、文件 artifact、设置分组、审查项。
- 不使用营销式 hero，不做欢迎大屏。
- 所有工具结果默认可折叠，大输出必须虚拟化或分页。
- 中文 UI 文案优先，技术名词保留英文。

需要后续补充：

- `PRODUCT.md`：用户、定位、产品原则、反例。
- `DESIGN.md`：颜色、字体、间距、组件、动效、空态。

## 8. MVP 范围

MVP 必须完成：

- 桌面应用可以启动。
- 可以选择工作目录。
- 可以创建新会话。
- 可以恢复已有会话。
- 可以发送消息。
- 可以流式显示 assistant 回复。
- 可以渲染基础工具卡片。
- 可以处理工具权限确认。
- 可以取消当前 turn。
- 可以显示文件编辑摘要。
- 可以打开设置页。
- 可以配置 provider/model 基础项。

MVP 明确不做：

- 完整 MCP 管理 UI。
- 完整插件市场。
- 完整自动化 UI。
- 完整 diff editor。
- 多窗口。
- 远程队列可视化。
- 子代理团队面板。
- 自动更新。

这些放到后续批次。

## 9. 分批任务

### 第 0 批：计划冻结

交付物：

- `docs/desktop-app-plan.md`
- UI 信息架构
- 事件协议草案
- Tauri command 草案
- MVP 验收标准

验收：

- 不改业务代码。
- 文档能指导下一批直接 scaffold。

### 第 1 批：Tauri + React scaffold

任务：

- 新增 `apps/yode-desktop`。
- 初始化 Tauri v2 + Vite + React + TypeScript。
- 接入 Tailwind、lucide-react、基础 lint/typecheck。
- 建立基础 app shell。
- 用 mock 数据实现 Codex 风格布局。

验收：

- `bun/npm/pnpm dev` 能启动前端。
- `cargo tauri dev` 或等价命令能启动桌面窗口。
- 主界面包含 sidebar、topbar、timeline、composer、settings shell。
- 不接真实 agent。

### 第 2 批：Rust desktop runtime

任务：

- 在 `src-tauri` 中实现 `DesktopRuntime`。
- 依赖现有 `yode-core` crates。
- 实现 session create/resume/list。
- 实现 `turn_send_message`。
- 把 `EngineEvent` 转为 `DesktopEvent` 并 emit 到前端。

验收：

- 前端能创建真实会话。
- 前端能发送消息并收到流式文本。
- 后端不通过 shell 调 CLI。

### 第 3 批：Timeline 和工具卡片

任务：

- 实现 timeline store。
- 渲染 user/assistant/system/tool 节点。
- 工具卡片支持 start/progress/result/error。
- Bash/File/Edit/Grep/Glob 做专门摘要。
- 大输出折叠。

验收：

- 一个正常 agent turn 可完整显示。
- 工具调用不会把 UI 撑坏。
- 失败结果有明确状态。

### 第 4 批：权限确认和审查

任务：

- 实现 tool confirmation UI。
- 支持 Allow / Deny。
- 支持文件编辑 diff summary。
- 支持权限模式显示和切换。
- 将确认结果回传 Rust。

验收：

- 需要确认的工具会阻塞并显示确认条。
- 用户确认后工具继续执行。
- 用户拒绝后模型收到结构化拒绝结果。

### 第 5 批：设置与项目管理

任务：

- 设置页接真实 config。
- provider/model 管理。
- 最近项目和最近会话。
- archive/rename session。
- 默认打开方式。
- 主题和语言设置。

验收：

- 常规设置可读写。
- 会话和项目导航可用。
- 退出重启后设置保留。

### 第 6 批：高级能力 UI

任务：

- MCP server 管理。
- Hooks 管理。
- 自动化/Cron 管理。
- 子代理和 task runtime 面板。
- Context/cost/status inspector。
- Git/worktree 面板。

验收：

- 高级 Yode 能力不再只能通过 slash command 暴露。
- 用户能在 UI 中理解当前 runtime 状态。

### 第 7 批：质量与发布

任务：

- 虚拟列表性能优化。
- 键盘快捷键。
- 错误/空态/i18n。
- Playwright 截图验证。
- macOS/Windows/Linux 打包。
- 自动更新策略。

验收：

- 长会话不卡顿。
- 基础截图回归稳定。
- 可以产出可安装桌面包。

## 10. 第一批前置决策

进入第 1 批前需要确认：

1. 包管理器：建议 `pnpm`，如果项目偏 Bun 也可用 `bun`。
2. 前端框架：建议 React，不建议 Vue/Svelte，原因是生态和桌面复杂 UI 组件更稳。
3. 样式：建议 Tailwind + CSS variables，不建议直接引入大型组件库。
4. 桌面路径：建议 `apps/yode-desktop`。
5. 首版平台：建议先 macOS，随后 Windows/Linux。
6. 桌面端是否保留 TUI：建议保留。

默认选择：

```text
package manager: pnpm
frontend: React + TypeScript + Vite
desktop: Tauri v2
style: Tailwind + CSS variables
icons: lucide-react
code viewer: CodeMirror 6
initial platform: macOS
```

## 11. 风险

- `AgentEngine` 当前主要服务 TUI，可能有 UI 假设，需要抽出更干净的 desktop runtime adapter。
- `EngineEvent` 部分类型未直接 `Serialize`，需要 DTO 转换。
- 多会话并发会引入 runtime ownership 和 cancellation 管理复杂度。
- Tauri command 不能传递任意 Rust 类型，必须建立稳定 JSON 协议。
- 大工具输出和长会话 timeline 必须虚拟化，否则桌面 UI 会卡。
- 文件权限、MCP、shell 执行在桌面端要更明确地暴露风险。
- 参考 Codex UI 时需要避免纯复制，应形成 Yode 自己的产品语言。

## 12. 下一步

下一步进入第 1 批：

1. 新增 `apps/yode-desktop`。
2. 初始化 Tauri v2 + React + TypeScript + Vite。
3. 建立 mock UI shell。
4. 运行桌面 dev server。
5. 用截图检查主界面布局。

第 1 批完成后，再开始 Rust runtime bridge。不要在 scaffold 阶段同时接真实 agent，否则 UI 和 runtime 两类问题会混在一起。
