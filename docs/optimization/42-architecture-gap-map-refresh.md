# Architecture Gap Map Refresh

## Current Strengths

- runtime task lifecycle, progress, retry, transcript backlinks
- review pipeline, commit gating, CI scaffold
- workflow runner with safe/write split and preview support
- coordinator dry-run phases and timeline rendering
- prompt-cache, system-prompt, compaction telemetry in `/status`
- MCP auth/cache/latency/reconnect diagnostics

## Remaining Architectural Gaps

### Orchestration

- multi-step workflow approval checkpoints are still command/tool level, not transaction-aware
- nested workflow invocation is blocked, but the UX around why/how to recover is still minimal

### Tool Runtime

- read history / command-vs-tool duplication / batch artifact views are still shallow
- hook timeline and tool timeline remain separate surfaces

### TUI

- narrow layout is improved, but there is no dedicated timeline/pager/dialog system
- review, transcript, and workflow previews are still text-first rather than panel-first

### Docs / Closeout

- final completion review and parity changelog need a consolidated pass once the remaining runtime items land

## Recommended Finish Order

1. orchestration safeguards: `008`, `009`
2. tool-runtime diagnostics: `075` through `080`
3. docs closeout: `098`, `099`, `100`
