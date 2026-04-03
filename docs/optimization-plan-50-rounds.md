# Yode 50 轮大优化计划

> 目标：让 Yode 媲美 Claude Code，从 113 个文件 → 500+ 文件级别的功能完整度
> 参考源：`~/code/claude/claude-code-rev` + `~/code/claude/opencode`

---

## 第一阶段：核心引擎 (轮次 1-10)

### 轮次 1: QueryEngine — 对话状态机
Claude Code 的 `QueryEngine.ts` 是核心：管理 idle→thinking→tool_use→done 的状态转换、token 预算、stop hooks。
- [ ] 重构 `engine.rs` 为 QueryEngine 状态机
- [ ] 新增 `query/transitions.rs` — 状态转换规则
- [ ] 新增 `query/token_budget.rs` — token 预算管理（按模型 context window 动态分配）
- [ ] 新增 `query/stop_hooks.rs` — 停止条件（max_turns, budget_exceeded, user_cancel）

### 轮次 2: Task 系统 — 后台任务框架
Claude Code 有 8 种 Task 类型，支持后台 shell、后台 agent、远程 agent。
- [ ] 新增 `crates/yode-tasks/` crate
- [ ] `LocalShellTask` — 后台 shell 命令（带 stall watchdog）
- [ ] `LocalAgentTask` — 后台 sub-agent
- [ ] `TaskManager` — 任务生命周期管理（create/list/get/stop/output）
- [ ] 新增工具：`task_create`, `task_list`, `task_output`, `task_stop`

### 轮次 3: Coordinator — 多 Agent 协调
Claude Code 的 coordinator 支持 workerAgent 并行执行、swarm 模式。
- [ ] 新增 `coordinator/mod.rs` — 协调器模式
- [ ] `worker_agent.rs` — Worker Agent 并行执行
- [ ] `SendMessageTool` — Agent 间通信
- [ ] `TeamCreateTool` / `TeamDeleteTool` — Agent 团队管理

### 轮次 4: 上下文注入增强
Claude Code 动态组装系统提示：git status、CLAUDE.md 层级加载、memory 文件、环境变量、diagnostics。
- [ ] `context/git_context.rs` — 完整 git 状态注入（branch, status, recent commits, remote info）
- [ ] `context/yode_md.rs` — 层级 YODE.md 加载（~/.yode/YODE.md → project/.yode/YODE.md → cwd/YODE.md）
- [ ] `context/memory_context.rs` — ~/.yode/memory/ 自动记忆系统
- [ ] `context/diagnostics.rs` — LSP diagnostics 注入到上下文

### 轮次 5: 系统提示工程
Claude Code 的系统提示极其精细，包含工具使用指南、代码风格、安全策略等。
- [ ] 重写 `prompts/system.md` — 完整的工具使用指南、代码风格、commit 规范
- [ ] `prompts/tools.md` — 每个工具的详细使用说明和示例
- [ ] 动态提示段：根据检测到的项目类型（Rust/Python/JS/Go）注入语言特定指引
- [ ] Effort level 控制输出详细度（min→max 映射到不同的提示段）

### 轮次 6: Session History — JSONL 持久化
Claude Code 的 `history.ts` 用 JSONL 格式存储完整对话历史，支持跨会话恢复。
- [ ] `session/history.rs` — JSONL 格式写入/读取
- [ ] `session/migration.rs` — 从 SQLite 迁移到 JSONL（或双写）
- [ ] `/resume` 命令增强 — 显示历史摘要、token 统计
- [ ] `/export` 命令 — 导出为 Markdown/JSON

### 轮次 7: compact / 上下文压缩增强
Claude Code 的 compact 很智能：区分系统消息、用户消息、工具消息的压缩策略。
- [ ] `services/compact.rs` — 智能压缩服务
- [ ] 用 LLM 生成摘要替代简单截断
- [ ] `/compact` 命令增强 — 显示压缩前后的 token 对比
- [ ] 自动 compact 阈值可配置

### 轮次 8: MCP 集成增强
Claude Code 有完整的 MCP 客户端：多服务器管理、OAuth 认证、SSE 传输、工具发现。
- [ ] `MCPTool` — 直接调用 MCP 服务器工具
- [ ] `McpAuthTool` — MCP OAuth 认证流程
- [ ] MCP 客户端支持 SSE 传输（除 stdio）
- [ ] `/mcp` 命令 — 管理 MCP 服务器连接状态

### 轮次 9: 文件编辑工具增强
Claude Code 的 FileEditTool 有 14 步验证流程，包括 fuzzy match、diff preview、undo 支持。
- [ ] `edit_file.rs` — 增加 fuzzy match（old_string 近似匹配）
- [ ] diff preview — 编辑前显示 unified diff
- [ ] 文件备份 — 编辑前自动备份到 `.yode/backups/`
- [ ] `read_file.rs` — 支持 PDF、图片描述（multimodal）、Jupyter notebook

### 轮次 10: Bash 工具终极增强
Claude Code 的 BashTool 有 tree-sitter AST 分析、sandbox 模式、sed 拦截等。
- [ ] Sandbox 模式 — 限制写入范围到项目目录
- [ ] Sed/Awk 拦截 — 建议用 edit_file 替代
- [ ] 环境变量注入 — 工作目录、session ID
- [ ] 多 Shell 支持 — bash/zsh/fish/PowerShell 自动检测

---

## 第二阶段：Slash 命令 (轮次 11-20)

### 轮次 11: /commit — 智能 Git 提交
- [ ] `git diff` 分析 → LLM 生成 commit message
- [ ] conventional commits 格式
- [ ] 自动 stage 检测
- [ ] hook 支持（pre-commit）

### 轮次 12: /diff — 增强 diff 查看
- [ ] 语法高亮 diff
- [ ] 文件级别的 diff 折叠
- [ ] `/diff --staged` / `/diff HEAD~3`
- [ ] diff stats 统计

### 轮次 13: /review — 代码审查
- [ ] 分析当前 diff 并给出审查意见
- [ ] 安全漏洞检查 (OWASP top 10)
- [ ] 代码风格检查
- [ ] PR 审查模式

### 轮次 14: /branch — 分支管理
- [ ] 创建/切换/删除分支
- [ ] 分支对比
- [ ] PR 创建辅助
- [ ] merge conflict 辅助解决

### 轮次 15: /memory — 记忆管理
- [ ] 查看/搜索/删除记忆
- [ ] 分类：user/project/feedback
- [ ] 自动提取记忆（从对话中）
- [ ] 记忆关联搜索

### 轮次 16: /init — 项目初始化
- [ ] 检测项目类型
- [ ] 自动生成 YODE.md
- [ ] 推荐 MCP 服务器
- [ ] 设置 .yode/ 目录结构

### 轮次 17: /summary — 会话摘要
- [ ] 当前会话摘要
- [ ] 修改文件列表
- [ ] 关键决策记录
- [ ] 导出为 markdown

### 轮次 18: /plan — 计划模式增强
- [ ] 交互式计划编辑
- [ ] 计划执行跟踪
- [ ] `VerifyPlanExecutionTool` — 验证计划完成度
- [ ] 计划导出/导入

### 轮次 19: /stats — 会话统计
- [ ] 详细 token 用量分析
- [ ] 按工具分类统计
- [ ] 按时间分布统计
- [ ] 成本趋势图

### 轮次 20: /config — 配置管理命令
- [ ] 查看/修改所有配置
- [ ] `ConfigTool` — LLM 可以读取/修改配置
- [ ] 配置验证
- [ ] 配置导入/导出

---

## 第三阶段：TUI/UX 增强 (轮次 21-30)

### 轮次 21: VIM 模式
Claude Code 有完整的 vim 模式：motions, operators, text objects。
- [ ] `vim/motions.rs` — hjkl, w, b, e, 0, $, gg, G
- [ ] `vim/operators.rs` — d, c, y, p
- [ ] `vim/text_objects.rs` — iw, aw, i", a"
- [ ] `/vim` 命令切换模式

### 轮次 22: 输入增强 — 历史搜索 + 自动补全
- [ ] Ctrl+R 历史搜索（像 bash）
- [ ] 上下键历史浏览
- [ ] Tab 补全文件路径
- [ ] @ 补全文件引用

### 轮次 23: 输出样式增强
- [ ] Markdown 渲染（代码块语法高亮）
- [ ] diff 输出着色
- [ ] 表格渲染
- [ ] 折叠长输出

### 轮次 24: 通知系统
- [ ] 长任务完成通知（终端 bell + 系统通知）
- [ ] 后台任务状态变化通知
- [ ] 错误弹窗
- [ ] `/tips` — 使用技巧提示

### 轮次 25: 快捷键系统
Claude Code 支持自定义快捷键 (`~/.claude/keybindings.json`)。
- [ ] `keybindings.rs` — 可配置快捷键
- [ ] 默认快捷键集合（Ctrl+C 取消, Ctrl+L 清屏, Shift+Tab 模式切换）
- [ ] `/keys` 命令增强 — 显示所有绑定
- [ ] 快捷键冲突检测

### 轮次 26: 多面板布局
- [ ] 分屏：对话 + 文件预览
- [ ] 弹出面板：工具确认、diff 预览
- [ ] 浮动面板：帮助、统计
- [ ] 面板切换快捷键

### 轮次 27: 图片/文件拖放支持
- [ ] 终端图片显示（iTerm2/Kitty 协议）
- [ ] 文件路径拖放到输入
- [ ] 剪贴板图片粘贴
- [ ] 截图工具

### 轮次 28: 主题系统
Claude Code 支持主题切换。
- [ ] `theme.rs` — 主题定义（颜色、样式）
- [ ] 内置主题：dark, light, monokai, solarized
- [ ] `/theme` 命令切换
- [ ] 自定义主题支持

### 轮次 29: 进度条 + 动画
- [ ] 工具执行进度条
- [ ] 文件上传/下载进度
- [ ] 上下文压缩进度
- [ ] 更丰富的 spinner 动画

### 轮次 30: 无障碍 + 国际化
- [ ] i18n — 中文/英文完整翻译
- [ ] 屏幕阅读器支持
- [ ] 高对比度模式
- [ ] 宽字符正确渲染

---

## 第四阶段：高级功能 (轮次 31-40)

### 轮次 31: Plugin 系统
Claude Code 有完整的插件系统。
- [ ] `plugins/mod.rs` — 插件加载框架
- [ ] 插件 manifest (plugin.toml)
- [ ] 插件可注册：工具、命令、Hook
- [ ] `/plugin` 命令 — 安装/卸载/列出

### 轮次 32: Skill 系统增强
Claude Code 的 Skills 是可执行的提示模板。
- [ ] Skill 目录 (`~/.yode/skills/`, `.yode/skills/`)
- [ ] Skill 搜索 + 发现
- [ ] `DiscoverSkillsTool` — LLM 可发现并调用 skills
- [ ] Skill 导入/导出

### 轮次 33: 自动记忆提取
Claude Code 的 `extractMemories` 服务会自动从对话中提取有价值信息。
- [ ] `services/extract_memories.rs` — 对话结束时自动提取
- [ ] 分类存储：user preference / project fact / decision
- [ ] 去重 — 不重复存储相同信息
- [ ] 记忆过期 — 自动清理过时记忆

### 轮次 34: Away Summary — 离开摘要
Claude Code 离开后会生成摘要，下次进入时显示。
- [ ] `services/away_summary.rs` — 会话结束时生成摘要
- [ ] 下次进入显示 "上次你在做什么..."
- [ ] 关联 git 变化 — "你离开后有 3 个新提交"
- [ ] 智能恢复建议

### 轮次 35: LSP 集成增强
- [ ] 多语言 LSP 自动启动（rust-analyzer, tsserver, pyright, gopls）
- [ ] 实时 diagnostics 注入到对话上下文
- [ ] go-to-definition / find-references 增强
- [ ] hover information

### 轮次 36: Web Browser 工具
Claude Code 有 WebBrowserTool（基于 Playwright）。
- [ ] `WebBrowserTool` — 无头浏览器访问
- [ ] 截图 + OCR
- [ ] DOM 查询
- [ ] 表单填写

### 轮次 37: Proactive — 主动建议
Claude Code 会主动建议：修复 lint 错误、更新依赖、改进代码等。
- [ ] `proactive/mod.rs` — 主动建议框架
- [ ] 文件保存时自动检查
- [ ] git pre-commit 检查
- [ ] 依赖安全审计

### 轮次 38: Rate Limit 智能处理
- [ ] 多提供商自动切换（主提供商限流时切换到备用）
- [ ] 请求排队 + 重试
- [ ] 限流消息展示
- [ ] 用量预警

### 轮次 39: Analytics / Insights
- [ ] 本地使用统计
- [ ] 按项目统计 token 消耗
- [ ] 按工具统计调用频率
- [ ] `/insights` 命令展示

### 轮次 40: 安全增强
- [ ] 敏感文件检测（.env, credentials, keys）
- [ ] 阻止提交敏感内容
- [ ] 沙箱网络限制
- [ ] 审计日志

---

## 第五阶段：生态整合 (轮次 41-50)

### 轮次 41: IDE 集成 — VS Code 扩展
- [ ] LSP-based 扩展框架
- [ ] 内联代码建议
- [ ] 侧边栏对话
- [ ] 跨 IDE 通信协议

### 轮次 42: IDE 集成 — JetBrains 插件
- [ ] JetBrains 插件框架
- [ ] 与 VS Code 扩展共享协议
- [ ] 内联 diff 预览

### 轮次 43: Remote Session — SSH 支持
Claude Code 支持远程 SSH 会话。
- [ ] SSH 隧道连接
- [ ] 远程文件操作
- [ ] 远程 shell
- [ ] 会话同步

### 轮次 44: CI/CD 集成
- [ ] GitHub Actions 工作流
- [ ] GitLab CI 集成
- [ ] 自动 PR 审查
- [ ] 自动修复 CI 失败

### 轮次 45: 团队协作
- [ ] 共享 YODE.md 模板
- [ ] 团队记忆同步
- [ ] 代码风格同步
- [ ] 权限策略共享

### 轮次 46: 语音交互
Claude Code 有完整的语音输入/输出。
- [ ] 语音转文字（Whisper）
- [ ] 语音唤醒
- [ ] TTS 朗读响应
- [ ] 语音命令

### 轮次 47: 自我进化 — AutoDream
Claude Code 有 `autoDream` 服务：在空闲时自动优化 CLAUDE.md。
- [ ] `services/auto_dream.rs` — 空闲时自动学习
- [ ] 自动更新 YODE.md
- [ ] 自动发现常用工作流
- [ ] 自动优化提示

### 轮次 48: 性能优化
- [ ] 启动速度 < 100ms（懒加载）
- [ ] 内存使用优化（流式处理大文件）
- [ ] 并发工具执行优化
- [ ] 缓存系统（文件读取缓存、git 状态缓存）

### 轮次 49: 测试覆盖
- [ ] 单元测试覆盖率 > 80%
- [ ] 集成测试（模拟 LLM 响应）
- [ ] E2E 测试（模拟用户交互）
- [ ] 性能基准测试

### 轮次 50: 文档 + 发布
- [ ] 完整用户文档（中英双语）
- [ ] API 文档
- [ ] 贡献指南
- [ ] homebrew/cargo install 发布
- [ ] 演示视频

---

## 优先级排序

**紧迫度** (每轮约 1-2 天工作量)：

| 优先级 | 轮次 | 核心价值 |
|--------|------|----------|
| P0 紧急 | 1, 2, 4, 5 | 引擎核心 + 上下文 — 直接影响 AI 输出质量 |
| P1 高 | 3, 6, 7, 8, 9, 10 | 功能完整度 — 让 Yode 能做 Claude Code 能做的事 |
| P2 中 | 11-20 | 命令系统 — 提升日常使用效率 |
| P3 中 | 21-30 | UX — 提升使用体验 |
| P4 低 | 31-40 | 高级功能 — 差异化竞争力 |
| P5 远期 | 41-50 | 生态 — 长期价值 |

---

## 量化目标

| 指标 | 当前 | 目标 |
|------|------|------|
| 源文件数 | 113 | 500+ |
| 工具数量 | 34 | 55+ |
| Slash 命令 | 18 | 80+ |
| 单元测试 | 75 | 500+ |
| 提供商数量 | 31 | 31 (已完成) |
| 支持语言 | 2 (中/英) | 2+ |
| 启动时间 | ~200ms | <100ms |
