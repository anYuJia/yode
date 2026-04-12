# Next 100 Optimization Tracker Round 2

## Scope

第二轮 100 项优化在第一轮完成后继续推进，重点从“补齐 Claude Code Rev 核心能力差距”转向：

- shared model/runtime primitives
- context management and transcript ergonomics
- TUI interaction and rendering polish
- tool output readability
- remote / diagnostics depth
- startup / export / artifact reliability

状态标记：

- `[ ]` 未开始
- `[~]` 进行中
- `[x]` 已完成

当前进度：

- `61 / 100` 已完成

## 001-010 Shared Types / Context

- `[x]` 001 message estimated char count helper
- `[x]` 002 tool call estimated char count helper
- `[x]` 003 context manager shared message footprint helper rollout
- `[x]` 004 prompt/token estimation regression snapshot tests
- `[x]` 005 transcript render shared summary formatter
- `[x]` 006 runtime artifact link model shared struct
- `[x]` 007 startup summary parsing helper
- `[x]` 008 command output preview truncation helper
- `[x]` 009 UI badge formatting helper inventory
- `[x]` 010 shared duration formatting for diagnostics panels

## 011-020 Context / Memory

- `[x]` 011 context summary line builder extraction
- `[x]` 012 context compaction removal heuristics tests by role mix
- `[x]` 013 context token estimate fallback calibration helper
- `[ ]` 014 context summary artifact cross-link to turn artifact
- `[x]` 015 memory document preview formatter reuse
- `[x]` 016 transcript compare section stats helper split
- `[x]` 017 transcript compare large-file folding heuristics cleanup
- `[x]` 018 transcript target resolution diagnostics
- `[x]` 019 session memory freshness state helper reuse
- `[x]` 020 memory command render split by target kind

## 021-030 TUI Input / Layout

- `[x]` 021 input wrapping helper extraction
- `[x]` 022 queued input preview formatter extraction
- `[x]` 023 ghost text rendering helper extraction
- `[x]` 024 completion popup palette centralization
- `[x]` 025 completion candidate row formatter extraction
- `[x]` 026 turn status indicator formatter extraction
- `[x]` 027 chat header gradient / layout split
- `[x]` 028 wizard palette centralization
- `[x]` 029 tool confirm palette centralization
- `[x]` 030 input cursor position calculation tests

## 031-040 Tool Rendering / Scrollback

- `[x]` 031 tool metadata render helper split
- `[x]` 032 diff preview render helper split
- `[x]` 033 standalone tool result fold helper
- `[x]` 034 bash command preview folding helper
- `[x]` 035 write/edit preview fold helper
- `[x]` 036 tool summary key extraction helper
- `[x]` 037 scrollback role style palette adoption
- `[x]` 038 subagent tool-use fold helper
- `[ ]` 039 user/assistant code-line highlighting helper reuse
- `[x]` 040 scrollback long-line truncation helper coverage tests

## 041-050 Diagnostics / Export

- `[x]` 041 remote doctor artifact listing formatter split
- `[x]` 042 diagnostics export bundle includes startup artifacts index
- `[x]` 043 diagnostics export bundle includes doctor bundle references
- `[x]` 044 status artifact link dedup helper
- `[x]` 045 status tool/runtime summary line compaction helper
- `[x]` 046 brief command preview formatter extraction
- `[x]` 047 diagnostics overview render split
- `[x]` 048 latest artifact candidate selector helper extraction
- `[x]` 049 doctor local checks grouped by subsystem
- `[x]` 050 doctor remote checks grouped by subsystem

## 051-060 Startup / Provider

- `[x]` 051 startup artifact writer helper tests by kind
- `[x]` 052 tooling bootstrap failure artifact schema tidy-up
- `[x]` 053 provider capability summary parser for UI surfaces
- `[x]` 054 provider metrics render section in status
- `[x]` 055 startup summary append helper extraction
- `[x]` 056 provider bootstrap env/config source breakdown
- `[x]` 057 startup profile bundle manifest
- `[x]` 058 startup warmup artifact cross-links in status
- `[x]` 059 provider inventory doctor check
- `[x]` 060 MCP startup failure summary surfaced in doctor

## 061-070 Tools / Workflows

- `[ ]` 061 task_output follow-mode output formatter split
- `[ ]` 062 task_output transcript backlink helper extraction
- `[ ]` 063 workflow dry-run plan formatter extraction
- `[ ]` 064 workflow approval checkpoint formatter extraction
- `[x]` 065 workflow path resolution helper tests
- `[ ]` 066 review pipeline summary formatter extraction
- `[ ]` 067 review_then_commit summary formatter extraction
- `[ ]` 068 review artifact payload shared builder rollout
- `[x]` 069 test_runner command rendering helper extraction
- `[x]` 070 test_runner framework detection tests by ecosystem

## 071-080 Engine / Runtime Polish

- `[ ]` 071 turn runtime artifact schema tests
- `[ ]` 072 stream retry delay diagnostics formatting helper
- `[ ]` 073 stream watchdog state reset helper
- `[ ]` 074 engine event payload formatting helper for TUI
- `[ ]` 075 tool progress summary formatter extraction
- `[ ]` 076 repeated tool failure summary formatter extraction
- `[ ]` 077 permission decision UI summary formatter extraction
- `[ ]` 078 recovery breadcrumb folding helper
- `[ ]` 079 turn artifact status cross-link in brief command
- `[ ]` 080 runtime task artifact index reuse in export bundle

## 081-090 Product / UX Parity

- `[ ]` 081 compare current startup artifact bundle against Claude flow notes
- `[ ]` 082 compare current tool pool diagnostics against Claude UI expectations
- `[ ]` 083 compare current permission guidance against Claude prompt language
- `[ ]` 084 compare resume telemetry against Claude session storage views
- `[ ]` 085 compare background task detail views against Claude task UX
- `[ ]` 086 compare doctor bundle output against Claude support/debug needs
- `[ ]` 087 compare input layout behavior at narrow widths against Claude REPL
- `[ ]` 088 compare tool result folding against Claude transcript readability
- `[ ]` 089 compare status density against Claude compact runtime panel
- `[ ]` 090 round-2 gap map refresh after next 50 items

## 091-100 Final Round 2 Closeout

- `[ ]` 091 final round-2 startup polish review
- `[ ]` 092 final round-2 tool rendering polish review
- `[ ]` 093 final round-2 permission UX polish review
- `[ ]` 094 final round-2 resume UX polish review
- `[ ]` 095 final round-2 diagnostics polish review
- `[ ]` 096 final round-2 export artifact review
- `[ ]` 097 final round-2 remote doctor review
- `[ ]` 098 final round-2 tracker refresh after 75 items
- `[ ]` 099 final round-2 tracker refresh after 100 items
- `[ ]` 100 final round-2 parity review document
