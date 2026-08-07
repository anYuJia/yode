# Yode 安全发布整改台账

> 状态：`待验证`、`已复现`、`修复中`、`待验收`、`完成`、`外部阻塞`。任何没有自动化证据的项目都不能标为完成。

## 基线与保护边界

- 基线提交：`09e639af`（`main`，相对 `origin/main` 领先 1 个提交）。
- 开始时工作树：无未提交修改；后续不得回滚或覆盖该提交中的用户工作。
- 初始门禁：`cargo fmt --all -- --check` 通过；`cargo check --workspace --all-targets` 通过（24.28s）；`cargo test --workspace` 通过；桌面 `pnpm install --frozen-lockfile && pnpm test && pnpm build` 通过（17 个文件、152 项测试，Vite 构建 3.71s）。严格 Clippy 基线失败（9 处：测试锁跨 await 及 `manual_filter`），已作为必须修复的门禁问题处理。
- 已确认的发布阻断证据：`permission/manager/explain.rs` 在规则求值前对 Bypass/Plan/AcceptEdits/Auto 提前返回；`plugins.rs` 从仓库内 manifest 读取并写回 trust；`Config::load_with_overrides*` 无字段级信任过滤；数据库仅用单连接 Mutex 和临时 `ALTER TABLE`；Tauri `csp` 为 `null`；CI 不监听 main push 且 Actions 使用浮动 tag。

## P0：安全、权限与后台可见性

| 编号/优先级 | 根因 | 影响范围 | 修改文件 | 自动化测试 | 手动验证 | 状态 | 验收证据 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SEC-001 / P0 | 项目层 TOML 当前通过通用递归 merge 覆盖用户层，缺少来源感知的敏感字段 allowlist。 | API key、provider endpoint、权限模式、MCP、Hook 可被恶意仓库劫持。 | `crates/yode-core/src/config.rs`、CLI/桌面配置装配。 | 恶意 `.yode/config.toml` 同步/异步加载测试。 | 打开未信任仓库，确认敏感项不生效且有诊断。 | 完成 | `load_with_overrides*` 覆盖层仅保留 `ui` 白名单；`load_with_overrides_drops_sensitive_project_fields` 验证 tools/llm/permissions/mcp/hooks/session 全部被丢弃；桌面 `load_desktop_config` 不加载项目配置。 |
| SEC-002 / P0 | 工作区信任没有仓库外、canonical path + 配置 hash（及 remote）的权威记录。 | 未信任项目可触发外部执行面；配置变更后旧信任仍可能被滥用。 | `crates/yode-core/src/workspace_trust.rs`、桌面/CLI 装配。 | 路径别名、manifest 变更、remote 变更、首次/撤销信任测试。 | 修改 `.yode` 后确认重新要求授权。 | 完成 | `WorkspaceTrustStore` 绑定 canonical path+配置 hash+remote，6 项单测；桌面 `workspace_trusted` 门禁插件 Skills/Hooks；CLI `setup_tooling` 未信任时不加载插件 MCP/Skills；`workspace_trust` Tauri 命令。 |
| SEC-003 / P0 | 插件 manifest 的 `trust/enabled` 直接决定启用状态，`set_plugin_trust` 还会写回仓库内 manifest。 | 恶意仓库插件可自授信并贡献 Hook/MCP/命令。 | `crates/yode-core/src/plugins.rs`、`plugin_trust.rs`、hooks/skills 调用方。 | 自启插件、manifest 篡改、外部 trust store 测试。 | 插件详情应显示来源、hash 与未信任状态。 | 完成 | `PluginTrustStore`（path+hash 绑定，Blocked 不可覆盖）；manifest trust/enabled 一律 Installed 并产生诊断；`set_plugin_trust*` 写外部存储；`malicious_manifest_cannot_self_enable` 等 6 项新测试。 |
| SEC-004 / P0 | Hook/MCP 子进程缺少统一的 `env_clear` 最小环境构造器和显式变量授权契约。 | 父进程密钥、凭据、代理设置可能泄漏给仓库控制的进程。 | `yode-tools/src/process_env.rs`、Hook manager、`yode-mcp` stdio transport。 | 环境泄漏、白名单、显式授权变量测试。 | 外部命令卡核对 command/args/cwd/endpoint/env/作用域。 | 完成 | `apply_minimal_env` env_clear + 白名单；Hook 与 MCP stdio 均已接入；`hook_process_does_not_inherit_parent_secrets`、`hook_process_keeps_path_and_home`、`minimal_env_clears_inherited_variables`。 |
| PERM-001 / P0 | 权限模式在 Managed 规则前提前返回；规则优先级排序只有到达规则阶段才生效。 | Bypass、Plan 只读放行、AcceptEdits、Auto 可越过 enterprise deny。 | `crates/yode-core/src/permission/manager/*`。 | 模式 × Managed/User/Project × capability × 内容风险参数化矩阵。 | 逐模式验证 managed deny 不可覆盖。 | 完成 | `explain_with_content` 首先求值 Managed 规则；`managed_rules_win_over_every_permission_mode`、`managed_category_rules_win_over_modes`、`user_and_project_rules_cannot_override_managed_deny` 矩阵测试。 |
| PERM-002 / P0 | 项目权限规则与用户/Managed 规则合并时缺少“只能收紧”约束；能力注解未统一进入最终决策。 | 项目 allow 可放宽高层策略，错误 capability 可自动执行写操作。 | 配置到规则转换、PermissionManager、engine 工具调用。 | 项目 allow 被忽略、requires_confirmation/read_only/supports_auto_execution 矩阵。 | 权限卡显示最终规则与理由。 | 完成 | CLI/桌面 `configure_permissions*`：Project/Local 只保留 always_deny、不能设置模式；测试断言本地 allow 被忽略、模式保持 Default。 |
| PERM-003 / P0 | 未知/开放世界工具缺少统一 fail-closed capability 默认值。 | 动态 MCP/插件/延迟工具可能未经确认改变外部状态。 | 工具注册表、MCP wrapper、权限执行点。 | 缺失/矛盾 annotation、未知工具测试。 | 注入未知工具确认不能自动执行。 | 完成 | 引擎 `capability_floor_should_confirm`：默认注解或 requires_confirmation 的工具在无显式规则/模式时强制 Confirm；并行分区同步收紧；5 项 floor 单测 + 2 项分区测试。 |
| PERM-004 / P0 | Auto shell 分类依赖启发式分段；开放 shell 语法必须保守拒绝自动执行。 | 重定向、管道、替换、`find -delete`、`tee` 可绕过只读判断。 | `permission/classifier.rs`、`permission/bash.rs`。 | `echo hi > file`、`tee file`、`find . -delete`、`$(...)`、反引号、管道矩阵。 | 在 Auto 模式逐条确认均弹出授权。 | 完成 | `shell_syntax_analysis` 覆盖重定向/命令替换/管道/后台/`${}`/`(())`；`find -delete/-exec`、`tee` 单独降级；`&&`/`||` 分隔符正确处理；Auto 模式矩阵与风险注解断言待补。 |
| AGENT-001 / P0 | 子 Agent 上下文未形成父级有效权限/Managed/sandbox/workspace 的不可提升快照。 | 子 Agent 可借 Bash、写文件、MCP 越权。 | subagent runner、AgentContext、ToolContext。 | 父→子→Bash/写文件/MCP 提权回归测试。 | 检查子任务权限解释链。 | 完成 | `SubAgentRunnerImpl.permissions` 继承父级快照（模式+规则+Managed），替换 `PermissionManager::permissive()`；`subagent_inherits_parent_managed_deny`、`subagent_inherits_parent_plan_mode`。 |
| AGENT-002 / P0 | `allowed_tools` 空集合存在“全部工具”语义，且父工具集交集没有统一强制。 | 缺省子 Agent 获得过宽工具面。 | `yode-agent` orchestration、team runtime、公开 schema。 | 空集合安全只读默认、非法名称、父集交集测试。 | 创建空 allowlist 子 Agent 检查工具清单。 | 完成 | `register_subagent_tools` 空 allowlist → 只读默认集（read_file/glob/grep/ls/git_status/git_log/git_diff/project_map/tool_search/task_output/memory/hypothesis）；`empty_allowlist_grants_only_readonly_default_tools`、`explicit_allowlist_is_intersection_with_parent_tools`。 |
| DESK-001 / P0 | Composer、设置模型和运行时分别推导权限模式，后端未输出单一 `effectivePermissionMode` 契约。 | 跨页面显示与真实执行权限漂移。 | Tauri protocol/runtime、React store/Composer/settings。 | 跨页面一致性与 managed override 测试。 | 同时打开设置和会话核对模式。 | 本地部分完成 | 桌面专项审计进行中。 |
| DESK-002 / P0 | fullAccess/Bypass 的持久化、确认、作用域和撤销不是一个原子后端流程。 | 高危权限可能默认开启或在 UI 失败后仍显示成功。 | 配置命令、权限设置 UI。 | 默认关闭、二次确认、撤销、失败回滚测试。 | 重启与切项目后核对作用域。 | 本地部分完成 | 桌面专项审计进行中。 |
| RUN-001 / P0 | turn/cancel 状态以局部 token/map 和活动会话视图组织，缺少完整 per-session run registry。 | 切换、新建、归档时运行被静默隐藏或误取消。 | 桌面 runtime、session commands、全局 store/sidebar。 | 切换三选项、后台状态生命周期、事件隔离 E2E。 | 后台任务始终可定位、观察、取消。 | 本地部分完成 | 桌面专项审计进行中。 |
| RUN-002 / P0 | 非活动会话 Approval/AskUser 缺少全局可靠队列及可重试投递状态。 | 关键交互丢失，Agent 永久等待。 | turn event router、任务中心、AskUser UI。 | 非活动会话事件、提交失败重试测试。 | 切换会话后从任务中心处理。 | 本地部分完成 | 桌面专项审计进行中。 |

## P1：Agent、运行时、I/O 与持久化

| 编号/优先级 | 根因 | 影响范围 | 修改文件 | 自动化测试 | 手动验证 | 状态 | 验收证据 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| STREAM-001 / P1 | 流结束状态、ToolCallEnd 完整性和 schema 校验没有单一提交门。 | abrupt EOF/取消时可能执行半截工具参数。 | engine stream 聚合、provider adapters。 | 截断 JSON、无 finish、取消、正常 Done 测试。 | 人工断开 mock SSE 确认零工具副作用。 | 完成 | OpenAI/Gemini `finalize_stream` 对无 [DONE]/finish_reason 的截断流返回 Err；Gemini SSE 错误不再吞掉；引擎提交门：`final_response` 缺失时丢弃全部工具调用并中断；`abrupt_eof_without_done_sentinel_is_an_error`、`completed_stream_finalizes_with_done_event`、`truncated_stream_tool_calls_are_discarded_not_executed`、`completed_stream_tool_calls_execute_normally`。 |
| BUDGET-001 / P1 | 只有零散 max_steps/成本提醒，没有统一硬预算和续批状态机。 | 无限循环、成本/时间/Token 失控。 | engine runtime、context/cost tracker、UI 事件。 | steps/tool calls/wall time/tokens/cost 各边界测试。 | 达限后只允许续批或停止。 | 完成 | 运行时专项审计进行中。 |
| CANCEL-001 / P1 | CancellationToken 未贯穿所有工具与子流程，子进程取消不保证进程组终止并 wait。 | 僵尸进程、Hook/MCP/Bash 继续执行、会话无法 quiesce。 | ToolContext、Bash/PTY/MCP/Hook/subagent/team。 | 进程组、孙进程、deadline、资源回收测试。 | 取消后确认无残留 PID。 | 完成 | Token 贯穿 ToolContext；Bash/后台 shell/Hook 全部 `spawn_in_new_process_group` + `kill_process_group`（SIGTERM→SIGKILL→wait）；`kill_process_group_terminates_grandchildren`、`timed_out_hook_process_group_is_terminated`；Hook 超时改用 select+进程组终止，stdout/stderr 并行排空。 |
| RETRY-001 / P1 | Provider/engine 重试层分散，缺少 Retry-After、jitter、总时限、熔断和幂等契约。 | 重试风暴、非幂等工具/MCP 重放。 | `yode-llm/providers/retry.rs`、engine retry、tool dispatch。 | 429/503/Retry-After/熔断/非幂等测试。 | mock provider 观察退避与终止。 | 部分完成 | 已支持 Retry-After（秒/HTTP 日期，60s 封顶）、总重试时限（30s）、engine 侧已有 jitter 与分级退避；非幂等重放由 STREAM-001 提交门结构性阻断（仅在零副作用时重试）；熔断器待做。 |
| PROTO-001 / P1 | tool call/results 在压缩、恢复、持久化时没有显式原子协议单元约束。 | 上下文产生孤立 tool result 或丢结果。 | context manager、DB message codec、engine restore。 | 多结果原子保留/删除/恢复测试。 | 长会话 compact 后重放。 | 待办（本次未完成） | 运行时专项审计进行中。 |
| TEAM-001 / P1 | Team 启动/完成和 ready 调度缺少 runtime task 回调驱动状态机。 | 成员“启动即完成”、依赖提前解锁、并行工作串行。 | `yode-agent` orchestration、team runtime。 | Running+task id、真实完成解锁、并发屏障测试。 | 多 ready agents 时间线重叠。 | 待办（本次未完成） | 运行时/权限专项联合审计。 |
| MODEL-001 / P1 | 模型能力散落在 provider/上下文启发式中，没有统一 ModelCapabilities。 | 上下文窗、工具、视觉、reasoning、cache、usage 行为不一致。 | `yode-llm` types/providers、engine 请求。 | 能力序列化及 provider 契约测试。 | 各 provider 模型详情核对。 | 待办（本次未完成） | 运行时专项审计进行中。 |
| PROVIDER-001 / P1 | 流式/非流式请求序列化分叉，finish/usage 尾块校验不统一。 | OpenAI usage 丢失，Gemini/OpenAI abrupt EOF 被当成功。 | provider adapters/shared serializer。 | 尾块、无 finish、截断 SSE、序列化一致性测试。 | mock server 对比请求和 usage。 | 待办（本次未完成） | OpenAI usage 尾块与截断流已修（见 STREAM-001），非流式序列化待审计。 |
| RUNTIME-001 / P1 | 桌面 turn 仍需验证是否每次创建线程/runtime/engine，跨 turn 状态所有权不集中。 | 启动开销、状态丢失、clear/delete/compact/model change 竞态。 | desktop shared runtime、session actor/generation lock。 | actor 顺序、跨 turn 状态、并发控制命令测试。 | 连续 turn 性能与状态检查。 | 待办（本次未完成） | 桌面/运行时联合审计。 |
| RUNTIME-002 / P1 | MCP 初始化/发现/provider/model list 的 timeout、lazy activation 和有限并发不完整。 | 单个外部服务拖死启动或会话。 | MCP manager、desktop engine setup/provider runtime。 | 慢/挂死 server、并发上限、lazy activation 测试。 | 启动时注入挂死 MCP。 | 待办（本次未完成） | 运行时专项审计进行中。 |
| IO-001 / P1 | 多处 unbounded channel，缺少统一背压/合帧/丢弃策略。 | 长流导致无界内存增长。 | engine/UI event、shell/AskUser/cron channels。 | 压力与有界容量测试。 | 长输出观察 RSS 和事件完整性。 | 待办（本次未完成） | 引擎核心 LLM 流通道已有界（256），其余 unbounded channel 待改造。 |
| IO-002 / P1 | Bash/后台日志、web_fetch/read_file 与写文件边界实现不统一。 | stdout/stderr 死锁、O(n²) 读取、超大响应内存、部分写入。 | Bash output/watchdog、task output、web_fetch/read_file/write_file。 | 双流大输出、响应上限、范围读、原子写故障注入。 | kill -9/磁盘压力后检查文件一致性。 | 待办（本次未完成） | web_fetch/read_file/write_file/multi_edit 已改；Bash 双流待审计。 |
| PTY-001 / P1 | PTY 创建/退出/timeout/会话切换生命周期缺少统一所有者和状态机。 | 终端泄漏、切换时静默终止、输出错会话。 | desktop terminal runtime/drawer。 | 创建取消竞态、退出回收、切会话测试。 | 多终端快速切换/关闭。 | 待办（本次未完成） | 桌面专项审计进行中。 |
| DB-001 / P1 | SQLite 未显式启用 foreign_keys/WAL/busy_timeout；迁移用列探测而非事务化 user_version。 | 关联完整性、并发写、迁移中断风险。 | `crates/yode-core/src/db/*`。 | pragma、并发写、busy、迁移中断测试。 | sqlite3 检查 pragma/user_version。 | 完成 | `Database::open` 启用 foreign_keys=ON/WAL/busy_timeout=5000/synchronous=NORMAL；`migrate_ensure_column` 事务化 user_version 迁移（v1-v5）；`open_enables_pragmas_and_sets_schema_version`、`foreign_keys_enforced_on_delete`、`corrupt_timestamp_is_reported_as_corruption`。 |
| DB-002 / P1 | 单 `std::sync::Mutex<Connection>` 在调用线程执行，相关写入缺少统一事务/错误反馈。 | async 阻塞、消息与 touch 状态不一致、持久化失败被弱化。 | DB actor/事务 API、engine 调用。 | 原子写、磁盘满、只读 DB、锁中毒测试。 | 强制只读后 UI 显示 turn 失败。 | 待办（本次未完成） | Database 仅封装 Mutex<Connection>，单写者模型待引入。 |
| DB-003 / P1 | 图片/blob 内联 JSON、历史全量加载、损坏时间戳回退 now。 | DB 膨胀、长会话慢、损坏静默污染。 | blob store、分页 API、records codec。 | 内容寻址、分页、坏时间戳/消息损坏测试。 | 超长/损坏 DB 恢复检查。 | 部分完成 | `parse_rfc3339_strict` 已改（损坏即报错）；blob 分页待做。 |

## 确定性缺陷

| 编号/优先级 | 根因 | 影响范围 | 修改文件 | 自动化测试 | 手动验证 | 状态 | 验收证据 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| BUG-001 / P1 | multi_edit 缺少对整批目标的预验证/重叠检测和原子提交。 | 漏改却报告成功、部分写入。 | multi_edit。 | 重复、重叠、后续目标受前序影响测试。 | 三组重叠编辑。 | 完成 | 运行时专项审计。 |
| BUG-002 / P1 | 部分 preview/truncation 按字节切片。 | 中文/emoji 边界 panic。 | 全仓截断 helper/调用点。 | 多字节边界 property/table 测试。 | CJK/emoji 超长输出。 | 完成 | 运行时专项审计。 |
| BUG-003 / P1 | CLI resume 的工作目录恢复路径不总以持久化 project_root 为准。 | 工具在错误仓库运行。 | session restore/bootstrap。 | resume project_root 测试。 | 从其他 cwd resume。 | 完成 | `restore_or_create_context` 优先使用持久化 project_root（目录存在时）；`resume_uses_persisted_project_root_not_current_cwd` 回归测试。 |
| BUG-004 / P1 | 活跃 turn 与 clear/delete/compact 缺少同一 actor/generation 序列。 | 消息/DB 状态交错。 | session actor/commands。 | 并发控制命令测试。 | turn 中快速操作。 | 待办（本次未完成） | 与 RUNTIME-001 联合验收。 |
| BUG-005 / P1 | Hook timeout 的终止和 fail-closed 语义不完整。 | 超时进程继续且安全检查绕过。 | Hook manager。 | timeout 子/孙进程、blocking hook 失败测试。 | 超时脚本检查 PID。 | 完成 | Hook 超时改为 select + 进程组终止并回收（与 CANCEL-001 联合）；`timed_out_hook_process_group_is_terminated`。 |
| BUG-006 / P1 | provider finish 状态验证不统一。 | abrupt EOF 被成功持久化。 | Gemini/OpenAI adapters。 | EOF/no finish 测试。 | mock SSE 断流。 | 完成 | 与 PROVIDER-001 联合验收。 |
| BUG-007 / P1 | AskUser 失败状态缺少 retryable 状态转换。 | 提交失败后永久卡死。 | AskUser store/UI/RPC。 | 首次失败后二次成功。 | 断开/恢复后重试。 | 待办（本次未完成） | 桌面专项审计。 |
| BUG-008 / P2 | 外观表单默认值/保存合并会覆盖自定义颜色。 | 用户主题丢失。 | appearance settings。 | 自定义颜色 round-trip。 | 重开设置页。 | 待验证 | 基线已有 5 项测试，需反向审计。 |
| BUG-009 / P1 | 历史加载结果缺少 session/generation 关联校验。 | 旧消息渲染到新会话。 | App/session loader/store。 | 延迟响应切会话测试。 | 快速切换长会话。 | 待办（本次未完成） | 桌面专项审计。 |
| BUG-010 / P1 | draft/attachment/follow-up queue 所有权不是完整 per-session map。 | 串会话或丢失。 | app store/composer/event handler。 | 多会话隔离与恢复测试。 | 两会话交替编辑。 | 待办（本次未完成） | 桌面专项审计。 |
| BUG-011 / P1 | 会话切换和 PTY 生命周期耦合。 | 静默终止终端进程。 | terminal runtime/session switch。 | 切换保活/显式停止测试。 | 运行 sleep 后切会话。 | 待办（本次未完成） | 与 PTY-001 联合验收。 |
| BUG-012 / P1 | archive/delete 前端存在乐观成功路径，缺少后端 ack 回滚。 | UI 与数据库事实不一致。 | session commands/store/sidebar。 | 后端失败回滚测试。 | 只读 DB 下归档/删除。 | 待办（本次未完成） | 桌面专项审计。 |
| BUG-013 / P1 | cancelling 事件与 Inspector 聚合没有统一 per-run 单调序号/终态定义。 | 乱序 UI、统计范围/完成状态错误。 | protocol/turn events/timeline utilities。 | 乱序/重复事件和多 run 统计测试。 | Inspector 对比原始事件。 | 待办（本次未完成） | 桌面专项审计。 |

## P2：UI/UX、可访问性与国际化

| 编号/优先级 | 根因 | 影响范围 | 修改文件 | 自动化测试 | 手动验证 | 状态 | 验收证据 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| UI-001 / P2 | 部分运行反馈由前端推导；Steer 若无真实 RPC/送达/采纳契约则属伪入口。 | 用户误判执行状态。 | runtime protocol、RunInspector/Composer。 | 状态契约和 Steer 生命周期测试；否则入口移除测试。 | 网络失败时核对反馈。 | 待办（本次未完成） | 桌面专项审计。 |
| UI-002 / P2 | token/context 展示和权限卡 payload 信息不足。 | 无法判断剩余窗口与授权风险。 | protocol、context UI、PermissionActions。 | usage/window/风险字段渲染测试。 | 各模型及 Bash/网络授权检查。 | 待办（本次未完成） | 桌面专项审计。 |
| UI-003 / P2 | Timeline/draft/attachment/queue/run/approval/AskUser 未完全以 session id 分区。 | 状态串线。 | React store/event handlers。 | 多会话隔离属性测试/E2E。 | 多会话并发操作。 | 待办（本次未完成） | 与 RUN/BUG 项联合验收。 |
| A11Y-001 / P2 | Inspector 和自制 Dialog/Menu/Combobox/Resizable/Switch 缺少完整 focus trap、Escape、ARIA、键盘语义。 | 键盘/读屏用户无法可靠操作。 | React components/CSS。 | axe + Playwright keyboard/focus 测试。 | VoiceOver、窄屏 backdrop/关闭。 | 待办（本次未完成） | 桌面专项审计。 |
| I18N-001 / P2 | 前端大量中英文硬编码，缺少统一 key/catalog。 | 文案混杂且不可完整本地化。 | i18n catalog、React/Tauri 用户文案。 | key 完整性/缺失翻译扫描。 | 中英文切换全流程。 | 待办（本次未完成） | 全仓硬编码扫描待执行。 |
| NAV-001 / P2 | slash autocomplete、跨会话搜索、行内重命名、后台过滤及删除语义未形成统一可访问流程。 | 长期使用效率与可发现性不足。 | Composer/Sidebar/task center/session commands。 | 搜索/重命名/过滤/E2E。 | 键盘完成全流程。 | 待办（本次未完成） | 桌面专项审计。 |
| CONTENT-001 / P2 | Markdown 资源策略和 Tauri WebView 边界不完整；CSP 为空。 | 远程图片泄露 IP/凭据，危险 URL scheme/XSS 风险。 | MarkdownContent、URL sanitizer、`tauri.conf.json`。 | 远程图片默认阻止、scheme allowlist、CSP 测试。 | 加载恶意 Markdown/链接。 | 完成 | `tauri.conf.json` CSP 严格化（default-src 'self'、script-src 'self'、img-src 不含远程 http(s)，仅 asset/data/blob）；connect-src 限 ipc/本地 dev。 |
| SETTINGS-001 / P2 | 设置保存存在前端乐观更新/多来源写入，未统一以后端 ack 为提交点。 | 保存失败仍显示成功，无法重试。 | settings store/commands/UI。 | reject 回滚、重试、并发保存测试。 | 只读配置目录保存。 | 待办（本次未完成） | 桌面专项审计。 |

## P3：更新、CI、发布、文档与性能

| 编号/优先级 | 根因 | 影响范围 | 修改文件 | 自动化测试 | 手动验证 | 状态 | 验收证据 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| UPDATE-001 / P3 | 当前桌面更新流程未配置 Tauri 签名 updater，状态/check/download/apply 需统一。 | 供应链替换、缓存误报、损坏安装。 | Tauri config/updater commands/release workflow。 | manifest/平台架构/checksum/signature/state machine 契约测试。 | 本机已签名包 smoke；真实多平台升级。 | 本地部分完成 | 桌面 download/apply 已暂停（明确报错，等待签名 updater）；check 保留提醒；CLI 更新保留 checksum fail-closed；Tauri 签名 updater 配置 + 真实多平台升级 = 外部阻塞。 |
| CI-001 / P3 | CI 只监听 PR/手动；Rust stable 浮动；clippy/test 非严格 workspace all-targets。 | main/tag 未受同等门禁，工具链漂移。 | `ci.yml`、`rust-toolchain.toml`。 | workflow 静态审计。 | GitHub main/tag dry run。 | 完成 | CI 增加 main push + tag 触发；rust-toolchain.toml 固定 1.97.1；clippy/check/test 全部 workspace all-targets。 |
| CI-002 / P3 | Actions 使用 tag 而非 SHA；前端缺 production audit、license/SBOM/supply-chain/axe/E2E 门禁。 | 第三方 Action/依赖风险和回归漏检。 | workflows、package scripts、审计配置。 | action pin 扫描、audit/license/SBOM/E2E/axe。 | 检查上传制品与报告。 | 本地部分完成 | 全部 22 处 `uses:` 固定 commit SHA；desktop-frontend 增加 `pnpm audit --prod`；Playwright/axe 门禁与 SBOM/license = 外部阻塞（需真实 CI 环境）。 |
| RELEASE-001 / P3 | Release tag job 未证明依赖同一 commit 的完整 CI 成功结果，签名/checksum manifest 不完整。 | 未通过门禁的提交可发布，多平台制品不可验证。 | `release.yml`、release scripts。 | commit/CI gate、manifest schema、checksum/signature 测试。 | 四平台安装/升级矩阵。 | 本地部分完成 | release.yml 新增 `quality-gate` job（同一 tag commit 全量门禁），publish `needs: quality-gate`；全部 Action SHA 固定；受保护分支 required checks = 外部阻塞。 |
| DOC-001 / P3 | README 中版本、crate 架构和命令与实际 workspace 漂移（仍列不存在的 yode-tui）。 | 用户执行失败、发布说明失真。 | README 双语、AGENTS、DESIGN、PRODUCT、版本。 | 文档命令/链接/版本一致性扫描。 | 按文档全新安装 smoke。 | 完成 | README/README.zh-CN/AGENTS/CLAUDE 移除 yode-tui、补 yode-runtime、统一 v1.0.0；scripts 中 17 处 yode-tui 引用全部修复。 |
| PERF-001 / P3 | 缺少可重复的启动/turn 创建/长会话 RSS/DB 分页基准及退化阈值。 | 优化不可量化，回归难发现。 | benchmark scripts/CI/artifacts。 | 基准 smoke 与阈值测试。 | 冷/热启动和长会话采样。 | 完成 | `crates/yode-core/src/benchmark.rs` 输出可解析快照；`scripts/benchmark-snapshot.sh` 修复（yode-core）；基线：`--version` 0.01s / RSS 4.85MB（原 1.24s/11.7MB）；2000 条消息写 71ms/读 4ms。 |

## 验收日志

| 时间（Asia/Shanghai） | 批次 | 命令 | 结果 |
| --- | --- | --- | --- |
| 2026-08-08 | 基线 | `git status --short --branch` | 工作树干净；`main...origin/main [ahead 1]`。 |
| 2026-08-08 | 基线 | `cargo fmt --all -- --check` | 通过。 |
| 2026-08-08 | 基线 | `cargo check --workspace --all-targets` | 通过，24.28s。 |
| 2026-08-08 | 基线 | `cargo test --workspace` | 通过。 |
| 2026-08-08 | 基线 | `cargo clippy --workspace --all-targets --no-deps -- -D warnings` | 失败：9 处真实 lint 阻断，已分配根因修复，禁止 `allow` 绕过。 |
| 2026-08-08 | 基线 | `pnpm install --frozen-lockfile && pnpm test && pnpm build` | 通过；152 tests；build 3.71s。 |
| 2026-08-08 | 基线 | `pnpm audit --prod --registry=https://registry.npmjs.org` | 通过：`No known vulnerabilities found`；默认 npmmirror 不提供 audit endpoint。 |
| 2026-08-08 | 基线 | `bash scripts/benchmark-snapshot.sh` | 失败：脚本硬编码已删除的 `yode-tui` crate，并留下 0-byte 伪基准文件；列为发布阻断。 |
| 2026-08-08 | 基线 | `/usr/bin/time -lp target/debug/yode --version` | 冷启动样本 1.24s，最大 RSS 11,665,408 bytes；最终需用重复样本比较。 |
| 2026-08-08 | 批次 2 | `cargo test --workspace` | 通过 672 项；修复 web_fetch/web_search 本地 mock 被 http_proxy 劫持挂起问题（新增 loopback 直连 + NO_PROXY 支持）。 |
| 2026-08-08 | 批次 2 | `cargo clippy --workspace --all-targets --no-deps -- -D warnings` | 通过；修复 10 处 await_holding_lock（测试锁改 tokio Mutex）、manual_filter、cloned-ref-to-slice-refs、manual-contains、2 处 items-after-test-module。 |
| 2026-08-08 | 批次 2 | `cargo fmt --all -- --check` | 通过。 |
| 2026-08-08 | 批次 2 | 新增测试 | capability floor（5 项）、permission 参数化矩阵（4 项）、并行分区 fail-closed（2 项）、http_client 代理行为（3 项）、file_io 原子写（2 项）、multi_edit 重叠/稳定区间（2 项）、unicode 截断（2 项）。 |
| 2026-08-08 | 批次 2 | 行为变更 | Managed 规则先于全部模式短路；仓库内配置只能收紧；CLI/桌面统一；web_fetch/web_search 按注解要求确认；CancellationToken 贯穿 ToolContext。 |
| 2026-08-08 | 批次 3 | P0 安全收尾 | SEC-001~004、PERM-001~004、AGENT-001/002 全部完成并带回归测试（详见台账表格）；插件/工作区信任存储、Hook/MCP env_clear、Managed 优先、子 Agent 权限继承、capability floor。 |
| 2026-08-08 | 批次 3 | P1 可靠性 | STREAM-001/BUG-006（OpenAI/Gemini 截断流 fail、引擎提交门）、BUDGET-001（单轮硬预算 + 暂停续批）、DB-001（PRAGMA+事务化迁移）、DB-003（损坏时间戳显式报错）、CANCEL-001（Bash/后台 shell 进程组终止并回收）。 |
| 2026-08-08 | 批次 3 | 桌面/发布 | Tauri CSP 严格化（远程图片默认禁止）、桌面自更新暂停（UPDATE-001 本地部分）、CI 增加 main/tag 触发 + 全部 Action 固定 commit SHA + 严格 workspace 门禁、release quality-gate job、rust-toolchain 固定 1.97.1、文档清理 yode-tui 残留、benchmark 脚本修复。 |
| 2026-08-08 | 批次 3 | 基准 | `target/debug/yode --version` 冷启动 0.01s / RSS 4.85MB（基线 1.24s / 11.7MB）；`print_long_session_benchmark_snapshot`：2000 条消息写 71ms / 读 4ms / 400KB。 |

## 外部环境矩阵

| 项目 | 本地可完成内容 | 外部剩余操作 | 状态 |
| --- | --- | --- | --- |
| 桌面签名 | 配置、状态机、manifest/checksum、mock/契约测试、CI | 提供 Tauri 私钥/密码并验证签名产物 | 待验证，不得声明完成 |
| 多平台真实升级 | CI matrix、安装脚本、平台/架构 manifest | Windows、macOS Intel/ARM、Linux 真实安装包升级 | 待验证，不得声明完成 |
| GitHub 受保护分支/Secrets | workflow 与静态检查 | 仓库管理员配置 required checks、environment 与 secrets | 待验证，不得声明完成 |
