# Golden Snapshot Storage Proposal

## Goal

Store parity snapshots in a deterministic, reviewable layout that can be copied from the working snapshot directory into a stable golden location.

## Layout

- Source: `.yode/benchmarks/`
- Golden target: `.yode/benchmarks/golden/current/`
- Required payloads:
  - `output-regression-snapshot.md`
  - `long-session-benchmark.md`
  - `output-regression-sections/`
  - `catalogs/`
  - `MANIFEST.md`

## Tooling

- `scripts/parity-golden-snapshot-store.sh`
- `scripts/parity-visual-diff.sh`

## Review Flow

1. Refresh working snapshots.
2. Run `scripts/parity-visual-diff.sh` against baseline and candidate.
3. If accepted, copy working snapshots into `golden/current`.
4. Review `MANIFEST.md` for stored payload inventory.
