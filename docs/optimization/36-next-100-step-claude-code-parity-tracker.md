# Next 100-Step Claude Code Parity Tracker

## Scope

这份文档用于跟踪 Yode 在完成首轮 100 项优化后，继续对齐 Claude Code 设计的下一阶段 100 项任务。

参考输入：

- `claude-code-docs/docs/01-Architecture-Overview.md`
- `claude-code-docs/docs/03-Agent-Loop.md`
- `claude-code-docs/docs/05-Compaction-System.md`
- `claude-code-docs/docs/06-Permission-System.md`
- `docs/optimization/32-top-tier-cli-parity-gap.md`

当前基线：

- 已具备 workflow runner、review pipeline、review-then-commit、coordinator、runtime tasks、memory/transcript artifacts、diagnostics overview
- 已补上 write-capable workflow runner、runtime task recent progress history、review shipping workflow templates
- 当前剩余大类差距集中在：coordinator orchestration、review/CI automation、background task streaming、prompt-cache telemetry、interactive transcript UX

状态标记：

- `[ ]` 未开始
- `[~]` 进行中
- `[x]` 已完成

当前焦点：

- `055 remote env setup verification command`

当前进度：

- `67 / 100` 已完成

## 001-010 Agent Loop / Orchestration

- `[x] 001 workflow write-capable runner`
- `[x] 002 runtime task recent progress history`
- `[x] 003 review shipping workflow templates`
- `[x] 004 coordinator dry-run dependency planner`
- `[x] 005 coordinator failure diagnostics with blocked dependency detail`
- `[x] 006 coordinator concurrency budget / phased parallelism`
- `[x] 007 sub-agent result artifact backlinks`
- `[ ] 008 workflow approval checkpoints for multi-step mutations`
- `[ ] 009 workflow nested invocation guard UX`
- `[x] 010 pipeline command presets for review / verify / ship`

## 011-020 Review / CI / Commit Flow

- `[x] 011 review pipeline presets for staged vs all changes`
- `[x] 012 review artifact status badges`
- `[x] 013 latest review summary in /status`
- `[x] 014 structured findings count metadata`
- `[x] 015 commit gate driven by review artifact status`
- `[x] 016 test command presets in /pipeline`
- `[x] 017 CI export command for review pipeline`
- `[x] 018 GitHub Actions scaffold for review gate`
- `[x] 019 review artifact diff backlinks`
- `[x] 020 multi-review session aggregation`

## 021-030 Task Runtime / Background Work

- `[x] 021 richer background bash progress streaming`
- `[x] 022 runtime task phase timestamps`
- `[x] 023 runtime task retry metadata`
- `[x] 024 runtime task artifact retention config`
- `[x] 025 /tasks latest / latest-by-kind shortcuts`
- `[x] 026 /tasks filters by kind and status`
- `[x] 027 /tasks tail follow mode`
- `[x] 028 notification severity classes for tasks`
- `[x] 029 background sub-agent output folding`
- `[x] 030 task-to-transcript backlinks`

## 031-040 Context / Prompt Cache / Memory

- `[x] 031 prompt-cache telemetry per turn`
- `[x] 032 cache hit / miss status surface`
- `[x] 033 system-prompt segment token breakdown`
- `[x] 034 compaction cause histogram`
- `[x] 035 memory freshness score in status`
- `[x] 036 live memory pending-update indicator`
- `[x] 037 transcript artifact preview folding`
- `[x] 038 context breaker mitigation hints`
- `[x] 039 resume-time cache warmup stats`
- `[x] 040 long-session benchmark command`

## 041-050 Permission / Safety / Recovery

- `[x] 041 permission explanation links to effective rule`
- `[x] 042 deny history grouped by tool`
- `[x] 043 bash safe-rewrite suggestions`
- `[x] 044 multi-step recovery breadcrumbs`
- `[x] 045 recovery state artifact`
- `[x] 046 plan-mode blocked-tool suggestions`
- `[x] 047 dangerous command rationale improvements`
- `[x] 048 permission hook snapshot export`
- `[x] 049 workflow write-mode confirmation summary`
- `[x] 050 safety-focused doctor checks`

## 051-060 MCP / Remote / Integration

- `[x] 051 MCP server health summary cards`
- `[x] 052 MCP auth status in /mcp`
- `[x] 053 MCP resource cache stats`
- `[x] 054 MCP tool latency telemetry`
- `[ ] 055 remote env setup verification command`
- `[ ] 056 remote review prerequisite diagnostics`
- `[ ] 057 MCP reconnect backoff diagnostics`
- `[x] 058 multi-server tool source badges`
- `[ ] 059 remote session artifact index`
- `[ ] 060 MCP / browser parity design notes`

## 061-070 TUI / UX / Narrow Layout

- `[ ] 061 transcript picker with folding`
- `[ ] 062 review artifact preview pager`
- `[ ] 063 workflow plan preview dialog`
- `[ ] 064 coordinator phase timeline widget`
- `[ ] 065 narrow-width status bar compaction`
- `[ ] 066 task badge compression for mobile-width terminals`
- `[ ] 067 history search preview polish`
- `[ ] 068 command suggestion ranking tweaks`
- `[ ] 069 attachment preview polish`
- `[ ] 070 theme / branding parity polish`

## 071-080 Tool Runtime / Diagnostics

- `[ ] 071 recent tool failure cluster detection`
- `[ ] 072 richer tool output preview formatting`
- `[ ] 073 diff preview line-budget telemetry`
- `[ ] 074 file-edit fallback explanation improvements`
- `[ ] 075 read-file history introspection`
- `[ ] 076 command vs tool duplication diagnostics`
- `[ ] 077 parallel tool batch artifacts`
- `[ ] 078 hook / tool combined timeline`
- `[ ] 079 tool budget soft warnings in TUI`
- `[ ] 080 diagnostics export bundle`

## 081-090 Release / CI / Performance

- `[x] 081 release preflight command`
- `[x] 082 version / tag consistency doctor check`
- `[x] 083 release note generation helper`
- `[ ] 084 benchmark snapshots in CI`
- `[x] 085 workflow template smoke tests in CI`
- `[x] 086 artifact index smoke test`
- `[x] 087 updater channel diagnostics`
- `[x] 088 package metadata consistency checks`
- `[x] 089 integration-test gating cleanup`
- `[x] 090 release checklist automation`

## 091-100 Docs / Tracking / Closeout

- `[x] 091 parity tracker weekly summary script`
- `[ ] 092 architecture gap map refresh`
- `[x] 093 workflow authoring guide`
- `[x] 094 coordinator usage guide`
- `[x] 095 review pipeline cookbook`
- `[x] 096 task runtime troubleshooting guide`
- `[ ] 097 prompt-cache telemetry doc`
- `[ ] 098 Claude Code parity changelog`
- `[ ] 099 next-100 completion review`
- `[ ] 100 top-tier parity delta refresh`
