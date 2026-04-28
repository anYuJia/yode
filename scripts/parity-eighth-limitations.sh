#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/288-eighth-limitations-note.md}"

cat >"$out_file" <<'EOF'
# Eighth Limitations Note

- CI jobs are wired, but no remote golden store backend exists yet.
- Replay storage is serialized fixture-based, not event-log replay.
- Artifact uploads are scoped to GitHub Actions and local bundle generation.
EOF

echo "Eighth limitations note written: $out_file"
