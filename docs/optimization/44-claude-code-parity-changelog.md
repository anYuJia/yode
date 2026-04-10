# Claude Code Parity Changelog

## Runtime / Tasks

- background tasks now track progress history, retry lineage, severity notifications, and transcript backlinks
- task output supports follow mode and folded long agent output

## Workflow / Coordinator

- workflow runner supports safe and write-capable modes
- workflow preview surfaces plan details before execution
- coordinator dry-run output includes a phase timeline

## Review / CI

- review artifacts include status badges, findings counts, diff backlinks, and aggregation
- review pipeline can export a GitHub Actions scaffold
- CI now runs core checks, artifact smoke verification, and long-session benchmark snapshot generation

## Context / Cache

- `/status` surfaces memory freshness, prompt-cache hit/miss state, system-prompt segment token estimates, and compaction cause histograms
- transcript previews and pickers fold long artifacts while preserving summaries

## MCP / Remote

- `/mcp` shows server summary, auth readiness, resource cache stats, tool latency, and reconnect/backoff diagnostics
- `/doctor remote`, `/doctor remote-review`, and `/doctor remote-artifacts` cover current remote-readiness diagnostics

## Remaining Work

- workflow approval checkpoints
- nested workflow guard UX
- read-history and command-vs-tool diagnostics
- hook/tool combined timeline
- final next-100 completion review
