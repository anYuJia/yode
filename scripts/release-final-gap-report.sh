#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/302-four-month-final-gap-report.md}"
mkdir -p "$(dirname "$out_file")"

cat >"$out_file" <<'EOF'
# Four-Month Final Gap Report

## Release Candidate Scope

- Product commands now cover the most useful Claude-equivalent operator flows: files, keybindings, context, diagnostics, permissions, MCP, skills, plugins, remote control, tasks, checkpoints, and review/workflow helpers.
- Long-session work now records compact boundaries, restore budgets, plan/task restore blocks, context mix summaries, and benchmark snapshots.
- CI parity work now has contract categories for command output, replay, visual, artifacts, and docs, with generated failure triage templates in uploaded bundles.
- Diagnostics are actionable enough for release-candidate support: top issues are grouped by severity, include quick actions, and point at inspectable artifacts where available.

## Accepted Non-Goals

- Full Claude-hosted remote product plane is not in scope for this release candidate; Yode keeps local-first remote/task artifacts and replay diagnostics.
- Model-native watch/monitor behavior is not claimed; current task and diagnostics flows expose operator-readable state and focused reruns.
- Server-managed or MDM policy delivery is not included; settings, permissions, MCP, and plugins remain repository/local/user scoped.
- Provider-specific tokenizer-perfect accounting is not claimed; context and restore budgets use deterministic local estimates.
- Full transcript re-execution from every serialized event is not required for this milestone; replay contracts remain anchored on focused fixtures and regression tests.

## Release Verification

- `cargo test --workspace --lib`
- `cargo clippy -p yode -p yode-core -p yode-llm -p yode-tools -p yode-tui -p yode-mcp -p yode-agent --no-deps -- -D warnings`
- `bash scripts/parity-ci-local.sh`
- `bash scripts/release-checklist.sh`

## Residual Risk

- CI contracts are now maintainable, but older parity scripts still need compatibility-wrapper deprecation before script sprawl is fully reduced.
- Long-session benchmark output should be compared again immediately before tagging so README/release notes do not overclaim context-survival improvements.
- Cross-platform confidence still depends on GitHub Actions for Linux/macOS/Windows workspace tests; local validation here is a release-candidate gate, not a full platform matrix.
EOF

echo "Final gap report written: $out_file"
