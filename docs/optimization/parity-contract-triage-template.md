# Parity Contract Failure Triage Template

Use this template from uploaded CI parity artifact bundles. Start with the contract that matches the failed job or artifact name, then run the listed focused scripts locally.

## CONTRACT-CMD (command-output)

- owner: command-output
- ci job: parity-snapshot
- uploaded artifact: parity-snapshot-artifacts
- triage doc: docs/optimization/parity-contract-triage-template.md
- artifact bundle: `.yode/parity-artifacts`
- manifest: `.yode/parity-artifacts/MANIFEST.md`

Fixtures / docs:
- `docs/optimization/parity-automation-manifest.tsv`
- `scripts/parity-command-audit.sh`

Focused reruns:
- `bash scripts/parity-command-audit.sh`
- `bash scripts/parity-snapshot-ci.sh`

Closeout fields:
- failed ci run:
- first failing command:
- artifact evidence:
- local rerun result:
- owner handoff:

## CONTRACT-REPLAY (replay)

- owner: transcript-rendering
- ci job: parity-replay
- uploaded artifact: parity-replay-artifacts
- triage doc: docs/optimization/parity-contract-triage-template.md
- artifact bundle: `.yode/parity-artifacts`
- manifest: `.yode/parity-artifacts/MANIFEST.md`

Fixtures / docs:
- `docs/optimization/267-eighth-replay-owner-map.md`
- `docs/optimization/277-eighth-replay-sample-export.json`

Focused reruns:
- `bash scripts/parity-replay-ci.sh`
- `bash scripts/parity-replay-storage-ci.sh`
- `bash scripts/parity-replay-index-ci.sh`

Closeout fields:
- failed ci run:
- first failing command:
- artifact evidence:
- local rerun result:
- owner handoff:

## CONTRACT-VISUAL (visual)

- owner: markdown-rendering
- ci job: parity-visual-docs
- uploaded artifact: parity-visual-docs-artifacts
- triage doc: docs/optimization/parity-contract-triage-template.md
- artifact bundle: `.yode/parity-artifacts`
- manifest: `.yode/parity-artifacts/MANIFEST.md`

Fixtures / docs:
- `docs/optimization/251-parity-visual-review-guide.md`
- `docs/optimization/261-parity-visual-inventory.md`

Focused reruns:
- `bash scripts/parity-visual-ci.sh`
- `bash scripts/parity-visual-hardening-audit.sh`
- `bash scripts/parity-visual-width-report.sh`

Closeout fields:
- failed ci run:
- first failing command:
- artifact evidence:
- local rerun result:
- owner handoff:

## CONTRACT-ARTIFACTS (artifacts)

- owner: artifact-governance
- ci job: parity-snapshot
- uploaded artifact: parity-snapshot-artifacts
- triage doc: docs/optimization/parity-contract-triage-template.md
- artifact bundle: `.yode/parity-artifacts`
- manifest: `.yode/parity-artifacts/MANIFEST.md`

Fixtures / docs:
- `docs/optimization/264-eighth-artifact-upload-policy.md`
- `docs/optimization/281-eighth-artifact-matrix-report.md`

Focused reruns:
- `bash scripts/parity-artifact-bundle.sh`
- `bash scripts/parity-artifact-retention-ci.sh`
- `bash scripts/parity-artifact-matrix-report.sh`

Closeout fields:
- failed ci run:
- first failing command:
- artifact evidence:
- local rerun result:
- owner handoff:

## CONTRACT-DOCS (docs)

- owner: docs-governance
- ci job: parity-visual-docs
- uploaded artifact: parity-visual-docs-artifacts
- triage doc: docs/optimization/parity-contract-triage-template.md
- artifact bundle: `.yode/parity-artifacts`
- manifest: `.yode/parity-artifacts/MANIFEST.md`

Fixtures / docs:
- `docs/optimization/243-parity-risk-register.md`
- `docs/optimization/244-parity-known-limitations.md`

Focused reruns:
- `bash scripts/parity-docs-ci.sh`
- `bash scripts/parity-risk-register-validate.sh`
- `bash scripts/parity-release-note-validate.sh`

Closeout fields:
- failed ci run:
- first failing command:
- artifact evidence:
- local rerun result:
- owner handoff:

