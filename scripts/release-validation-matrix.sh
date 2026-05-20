#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/306-release-validation-matrix.md}"
workflow="${2:-.github/workflows/ci.yml}"

[[ -f "$workflow" ]] || { echo "Missing CI workflow: $workflow" >&2; exit 1; }

rg -q 'os: \[ubuntu-latest, macos-latest, windows-latest\]' "$workflow"
rg -q 'run: cargo test --workspace --lib' "$workflow"
rg -q 'run: cargo clippy -p yode -p yode-core -p yode-llm -p yode-tools -p yode-tui -p yode-mcp -p yode-agent --no-deps -- -D warnings' "$workflow"
rg -q 'bash scripts/parity-snapshot-ci.sh' "$workflow"
rg -q 'bash scripts/parity-replay-ci.sh' "$workflow"
rg -q 'bash scripts/parity-docs-ci.sh' "$workflow"
rg -q 'parity-snapshot-artifacts' "$workflow"
rg -q 'parity-replay-artifacts' "$workflow"
rg -q 'parity-visual-docs-artifacts' "$workflow"
rg -q 'yode-benchmark-snapshot' "$workflow"

mkdir -p "$(dirname "$out_file")"
cat >"$out_file" <<'EOF'
# Release Validation Matrix

## CI Platform Coverage

- `rust` job runs format, clippy, cargo check, workspace library tests, audit, provider integration tests, compact artifact smoke verification, and benchmark snapshot upload on `ubuntu-latest`.
- `test-matrix` runs `cargo test --workspace --lib` on `ubuntu-latest`, `macos-latest`, and `windows-latest`.
- Parity jobs run snapshot, replay, visual/docs, and upload their parity artifact bundles.

## Required Release Gates

- `cargo test --workspace --lib`
- `cargo clippy -p yode -p yode-core -p yode-llm -p yode-tools -p yode-tui -p yode-mcp -p yode-agent --no-deps -- -D warnings`
- `bash scripts/parity-ci-local.sh`
- `bash scripts/release-checklist.sh`

## Uploaded Evidence

- `yode-benchmark-snapshot`
- `parity-snapshot-artifacts`
- `parity-replay-artifacts`
- `parity-visual-docs-artifacts`

## Release Interpretation

- Local release-candidate validation can confirm the current platform and release scripts.
- The final tag should wait for the GitHub Actions Linux/macOS/Windows matrix and parity artifact uploads to finish successfully.
EOF

echo "Release validation matrix written: $out_file"
