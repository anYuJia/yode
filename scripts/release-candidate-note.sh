#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/304-four-month-release-note-draft.md}"
mkdir -p "$(dirname "$out_file")"

cat >"$out_file" <<'EOF'
# Four-Month Release Candidate Note

## User-Visible Highlights

- Long sessions now preserve more restart context through compact boundary records, restore budgets, plan/task restore blocks, and `/context` mix summaries.
- Operator diagnostics are easier to act on: `/diagnostics` groups top issues by severity, includes quick-action hints, and points to inspectable artifacts when available.
- Product commands now expose more day-to-day surfaces directly, including `/files`, `/keybindings`, richer `/permissions`, MCP status/reload/resource policy views, skills, plugins, remote control, tasks, and checkpoint/review helpers.
- Remote/task flows have durable local artifacts, replay diagnostics, storage boundaries, and transport/queue summaries for support handoff.
- Release validation now has parity contracts for command output, replay, visual output, artifacts, and docs, plus generated failure triage templates in uploaded CI bundles.

## Compatibility And Upgrade Notes

- Existing config files continue to load through default merging; older files without `[update]` still receive update defaults.
- Permission governance remains explicit: managed/user/project/local scopes are documented, and mutating writes still require confirmation or dry-run-first command flows.
- The release candidate remains local-first. It does not claim Claude-hosted remote infrastructure, model-native monitor/watch behavior, or server-managed policy delivery.

## Verification Before Tagging

- `cargo test --workspace --lib`
- `cargo clippy -p yode -p yode-core -p yode-llm -p yode-tools -p yode-tui -p yode-mcp -p yode-agent --no-deps -- -D warnings`
- `bash scripts/parity-ci-local.sh`
- `bash scripts/release-checklist.sh`

## Known Limits

- Context and restore budget accounting is deterministic and local; provider-specific tokenizer-perfect accounting is not claimed.
- Replay contracts are focused regression fixtures and stored artifacts, not full transcript re-execution for every serialized event.
- Cross-platform release confidence still depends on the GitHub Actions Linux/macOS/Windows matrix before tagging.
EOF

echo "Release candidate note written: $out_file"
