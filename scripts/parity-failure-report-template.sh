#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/269-eighth-failure-report-template.md}"

cat >"$out_file" <<'EOF'
# Eighth Failure Report Template

- row:
- surface:
- owner:
- command:
- evidence:
- artifact bundle:
- next focused rerun:
EOF

echo "Parity failure report template written: $out_file"
