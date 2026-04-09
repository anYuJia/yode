# 100-Step Optimization Tracker

## Scope

这份文档从当前基线开始，跟踪 Yode 接下来 100 个可执行优化任务。

当前基线：

- Context / compact / transcript artifacts 检视能力已收口
- `/memory list` 已支持 mode / summary / failed / date-range / 组合过滤
- `/memory compare` 已支持 metadata compare 与 diff preview

状态标记：

- `[ ]` 未开始
- `[~]` 进行中
- `[x]` 已完成

当前焦点：

- `100 Top-tier CLI parity gap summary`

当前进度：

- `100 / 100` 已完成

## 001-010 Structured Memory

- `[x] 001 Structured live/session memory schema`
- `[x] 002 Live memory 增加 Decisions section`
- `[x] 003 Live memory 增加 Open Questions extraction`
- `[x] 004 Session memory entry schema 对齐 live memory`
- `[x] 005 Prompt 与 persisted memory schema 完全对齐`
- `[x] 006 Memory schema 增加 Files Read / Files Modified 分段`
- `[x] 007 Memory schema 增加 confidence / freshness 提示`
- `[x] 008 Compact summary 自动映射到 structured findings`
- `[x] 009 Memory schema 向后兼容旧 markdown 内容`
- `[x] 010 `/memory live` / `/memory session` 增加 schema-aware 展示`

## 011-020 Context Health Dashboard

- `[x] 011 `/context` 增加 compact count`
- `[x] 012 `/context` 增加 last breaker reason`
- `[x] 013 `/status` 增加 session memory update count`
- `[x] 014 `/status` 增加 last failed tool result count`
- `[x] 015 Engine runtime state 增加 compact counters`
- `[x] 016 Engine runtime state 增加 breaker telemetry`
- `[x] 017 TUI status line 增加 compact indicator`
- `[x] 018 TUI status line 增加 live-memory indicator`
- `[x] 019 Diagnostics 输出统一成 compact / memory / tools 三段`
- `[x] 020 `/doctor` 纳入 context/memory 健康检查`

## 021-030 Transcript / History

- `[x] 021 Transcript compare 增加 section-level summary`
- `[x] 022 Transcript compare 增加 content diff limits/flags`
- `[x] 023 Transcript compare 支持 latest 别名组合`
- `[x] 024 Transcript metadata 增加 session memory path`
- `[x] 025 Transcript metadata 增加 file-touch summary`
- `[x] 026 Transcript 目标解析支持更稳定的 fuzzy alias`
- `[x] 027 `/memory latest` 增加 compare shortcut`
- `[x] 028 Resume 后 transcript / memory artifact 索引重建`
- `[x] 029 `/sessions` 视图接入 latest transcript 摘要`
- `[x] 030 Session history 与 transcript artifacts 形成稳定关联`

## 031-040 Hooks / Async Lifecycle

- `[x] 031 Hook context 增加 compact counters`
- `[x] 032 Hook context 增加 live memory status`
- `[x] 033 Hook output 支持 memory-specific structured fields`
- `[x] 034 Async hook wake notification 基础框架`
- `[x] 035 Long-running hook timeout / cancel tracing`
- `[x] 036 Hook failure telemetry aggregation`
- `[x] 037 Session end hook 增加 memory flush metadata`
- `[x] 038 Permission hooks 增加 final effective input snapshot`
- `[x] 039 Hook additionalContext 注入优先级治理`
- `[x] 040 Hook diagnostics command`

## 041-050 Tool Runtime / Budget

- `[x] 041 Tool budget runtime counters`
- `[x] 042 Tool budget warnings 暴露到 `/status``
- `[x] 043 Large tool result 截断原因细化`
- `[x] 044 Tool result metadata 结构化 error type 展示`
- `[x] 045 Tool progress tracing 聚合`
- `[x] 046 Parallel tool execution telemetry`
- `[x] 047 Repeated-failure tool pattern detection`
- `[x] 048 Tool budget per-turn summary artifact`
- `[x] 049 Tool output diff-aware preview`
- `[x] 050 `/tools` diagnostics command`

## 051-060 Permission / Recovery

- `[x] 051 Denial tracker 基础实现`
- `[x] 052 Permission denied recent history 展示`
- `[x] 053 Exact failed signature UX 暴露`
- `[x] 054 Recovery state 对外诊断输出`
- `[x] 055 Reanchor-required UI hint`
- `[x] 056 Single-step recovery mode telemetry`
- `[x] 057 Permission classifier explanation surface`
- `[x] 058 Permission rules diagnostics`
- `[x] 059 Dangerous command fallback prompt hardening`
- `[x] 060 Recovery path tests for compact + hooks interaction`

## 061-070 Task Runtime / Agent Work

- `[x] 061 Unified task runtime data model`
- `[x] 062 Background task registry`
- `[x] 063 Task progress event channel`
- `[x] 064 Task output persistence`
- `[x] 065 Task stop / cancel support`
- `[x] 066 `/tasks` list command`
- `[x] 067 `/tasks <id>` inspect command`
- `[x] 068 Agent/sub-agent task state alignment`
- `[x] 069 Task notifications in TUI`
- `[x] 070 Task artifact retention policy`

## 071-080 TUI / UX

- `[x] 071 Memory panel style polish`
- `[x] 072 Compare output paging / folding`
- `[x] 073 Long artifact output scroll affordance`
- `[x] 074 Compact event UI grouping`
- `[x] 075 Session memory updated event grouping`
- `[x] 076 Error / failed transcript quick-jump`
- `[x] 077 Transcript selection dialog`
- `[x] 078 Unified diagnostics overview page`
- `[x] 079 Non-interactive command output consistency`
- `[x] 080 Mobile-width / narrow terminal formatting pass`

## 081-090 Performance / Reliability

- `[x] 081 Session memory render allocation cleanup`
- `[x] 082 Transcript metadata parsing cache`
- `[x] 083 Latest transcript lookup optimization`
- `[x] 084 Compact artifact write failure retry`
- `[x] 085 Resume path metadata rebuild benchmark`
- `[x] 086 Memory file truncation policy hardening`
- `[x] 087 Large transcript compare performance cap`
- `[x] 088 Prompt-cache / compact interaction telemetry`
- `[x] 089 Startup lazy-load for diagnostics paths`
- `[x] 090 Regression test matrix for long sessions`

## 091-100 Docs / Verification / Release

- `[x] 091 Structured memory design doc`
- `[x] 092 Context dashboard design doc`
- `[x] 093 Task runtime design doc`
- `[x] 094 Hook async wake design doc`
- `[x] 095 Release notes for memory/diagnostics line`
- `[x] 096 End-to-end verification script for compact artifacts`
- `[x] 097 Example project walkthrough doc`
- `[x] 098 Benchmark report for long-session behavior`
- `[x] 099 Final 100-step completion review`
- `[x] 100 Top-tier CLI parity gap summary`
