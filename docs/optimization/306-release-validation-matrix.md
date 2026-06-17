# Release Validation Matrix

## CI Platform Coverage

- `rust` job runs format, clippy, cargo check, workspace library tests, audit, provider integration tests, compact artifact smoke verification, and benchmark snapshot upload on `ubuntu-latest`.
- `test-matrix` runs CLI package library tests on `ubuntu-latest`, `macos-latest`, and `windows-latest`.
- Parity jobs run snapshot, replay, visual/docs, and upload their parity artifact bundles.

## Required Release Gates

- `cargo test $YODE_CLI_PACKAGES --lib`
- `cargo clippy $YODE_CLI_PACKAGES --no-deps -- -D warnings`
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
