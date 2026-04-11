# Claude Code Rev Next 100 Optimization Tracker

## Scope

这份文档是基于 [46-claude-code-rev-gap-analysis-and-fix-tracker.md](/Users/pyu/code/yode/docs/optimization/46-claude-code-rev-gap-analysis-and-fix-tracker.md) 的可执行 100 项优化清单。

目标不是照搬 Claude Code 的巨型入口或单体工具装配，而是在保留 `yode` Rust 多 crate 清晰边界的前提下，持续补齐：

- startup / warmup
- tool inventory / policy
- shell safety
- provider streaming
- session / resume / transcript
- TUI / diagnostics / UX
- workflow / review / background runtime

状态标记：

- `[ ]` 未开始
- `[~]` 进行中
- `[x]` 已完成

当前进度：

- `69 / 100` 已完成

## 001-010 Startup / Warmup

- `[x]` 001 startup phase profiler
- `[x]` 002 startup structured summary log
- `[x]` 003 startup profile surfaced in `/status`
- `[x]` 004 startup profile surfaced in `/doctor`
- `[x]` 005 skill discovery offloaded to blocking warmup
- `[x]` 006 MCP connect parallelized
- `[x]` 007 MCP discovery separated from registration
- `[x]` 008 database open overlapped with provider bootstrap
- `[x]` 009 provider bootstrap overlapped with tooling setup
- `[x]` 010 provider bootstrap metrics added to startup summary

## 011-020 Tool Inventory / Pooling

- `[x]` 011 tooling inventory counts in startup diagnostics
- `[x]` 012 tooling inventory surfaced in `/status`
- `[x]` 013 active vs deferred tool inventory snapshot API
- `[x]` 014 MCP vs builtin vs deferred tool breakdown in diagnostics
- `[x]` 015 session-time tool pool snapshot artifact
- `[x]` 016 tool pool changes tracked across startup and runtime activation
- `[x]` 017 deferred tool activation telemetry
- `[x]` 018 tool_search auto-enable reason surfaced to diagnostics
- `[x]` 019 tool registry duplicate-name guard diagnostics
- `[x]` 020 command/tool overlap detector

## 021-030 Shell / Permission

- `[x]` 021 bash policy helper module extracted
- `[x]` 022 bash auto-mode decision centralized
- `[x]` 023 bash discovery redirect logic centralized
- `[x]` 024 destructive bash guard message centralized
- `[ ]` 025 shell safety helper module split from generic permission manager
- `[x]` 026 shell rule explanation surface for pattern matches
- `[x]` 027 shell permission artifact includes classifier rationale field
- `[x]` 028 shell deny clustering by command prefix
- `[x]` 029 shell rule suggestion engine for repeated confirmations
- `[x]` 030 shell safe-readonly prefix inventory for diagnostics

## 031-040 Provider / Streaming

- `[x]` 031 shared assistant stream finalization helper
- `[x]` 032 anthropic streaming helper extraction
- `[x]` 033 openai provider request/response types split
- `[x]` 034 openai streaming helper extraction
- `[x]` 035 provider shared stream state trait / helper surface
- `[x]` 036 shared provider error-normalization helper
- `[x]` 037 shared stop-reason mapping helper
- `[x]` 038 shared usage-update emission helper
- `[x]` 039 shared tool-call delta aggregation helper
- `[x]` 040 provider startup capability summary in diagnostics

## 041-050 Session / Resume

- `[x]` 041 resume transcript warmup overlapped with TUI startup
- `[x]` 042 resume warmup progress staged into startup profile
- `[x]` 043 resume cache hit/miss counters by transcript metadata cache
- `[ ]` 044 resume path split between metadata-only and full transcript restore
- `[x]` 045 restored message decode metrics
- `[x]` 046 db open / load timing surfaced in startup profile
- `[x]` 047 transcript cache invalidation diagnostics
- `[x]` 048 session restore fallback reason reporting
- `[x]` 049 transcript benchmark summary exposed in `/doctor`
- `[ ]` 050 resume hot path regression test matrix

## 051-060 TUI Structure

- `[x]` 051 scrollback entry formatting split
- `[x]` 052 scrollback role formatters split
- `[x]` 053 chat entry renderers split
- `[x]` 054 markdown rendering split into block and inline layers
- `[x]` 055 provider wizard builders split
- `[ ]` 056 chat viewport rendering split into layout vs rendering phases
- `[x]` 057 status page rendering split into compact sections
- `[x]` 058 tasks/info renderers split from command handlers
- `[ ]` 059 narrow-width layout helper consolidation
- `[ ]` 060 shared TUI style palette module

## 061-070 Engine Runtime

- `[x]` 061 streaming turn runtime split into loop and finalization
- `[ ]` 062 turn cancellation path isolated from stream loop
- `[ ]` 063 protocol-violation retry helper isolated from stream finalization
- `[ ]` 064 tool-call execution continuation helper isolated from stream finalization
- `[ ]` 065 partial-stream recovery path tested independently
- `[ ]` 066 engine runtime timing aggregation per turn
- `[ ]` 067 stream watchdog diagnostics enriched with stage label
- `[ ]` 068 stream retry reason histogram
- `[ ]` 069 turn-complete artifact with response stop reason
- `[ ]` 070 engine startup vs turn-runtime metrics separated in status

## 071-080 Tool Modules

- `[x]` 071 workflow execution split
- `[x]` 072 project_map analysis split
- `[x]` 073 task_output execution split
- `[x]` 074 review_pipeline execution split
- `[x]` 075 review_then_commit execution split
- `[x]` 076 test_runner detection split
- `[ ]` 077 workflow variable application helper extraction
- `[ ]` 078 project_map language analyzers split by ecosystem
- `[ ]` 079 test_runner output parsing split by framework
- `[ ]` 080 review artifact shared formatter module

## 081-090 Diagnostics / Artifacts

- `[x]` 081 startup profile persisted as session artifact
- `[x]` 082 tooling inventory artifact emitted on startup
- `[x]` 083 provider inventory artifact emitted on startup
- `[x]` 084 MCP connect failure summary artifact
- `[x]` 085 permission policy summary artifact
- `[x]` 086 transcript cache warmup artifact
- `[x]` 087 runtime task inventory artifact in `/status`
- `[ ]` 088 doctor summary export bundle
- `[ ]` 089 review pipeline artifact schema cleanup
- `[x]` 090 tool execution artifact cross-links in status

## 091-100 Product / Parity Follow-up

- `[ ]` 091 direct compare against Claude startup sequence with measured deltas
- `[~]` 092 direct compare against Claude tool inventory gating
- `[ ]` 093 direct compare against Claude bash permission prompts
- `[ ]` 094 direct compare against Claude resume path and storage
- `[ ]` 095 direct compare against Claude prompt input ergonomics
- `[ ]` 096 direct compare against Claude background task UX
- `[ ]` 097 direct compare against Claude status/doctor diagnostics
- `[ ]` 098 final gap map refresh after next 25 items
- `[ ]` 099 final gap map refresh after next 50 items
- `[ ]` 100 final gap map refresh after next 100 items

## Current Focus

当前优先顺序：

1. `015-020` 工具池 artifact / activation telemetry / overlap detector
2. `092-097` 继续对齐 Claude 的工具 gating / 权限提示 / 诊断面
3. `034-039` provider streaming 共性继续统一
4. `062-070` streaming turn runtime 再收敛
