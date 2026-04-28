#!/usr/bin/env bash
set -euo pipefail

replay_dir="${1:-.yode/benchmarks/replay}"
out_file="${2:-docs/optimization/278-eighth-replay-summary-report.md}"
bash scripts/parity-replay-serialize.sh "$replay_dir" >/dev/null

python3 - "$replay_dir/replay-index.json" "$out_file" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())
fixtures = data.get("fixtures", [])
lines = [
    "# Eighth Replay Summary Report",
    "",
    f"- version: {data.get('version')}",
    f"- fixtures: {len(fixtures)}",
]
Path(sys.argv[2]).write_text("\n".join(lines) + "\n")
PY

echo "Parity replay summary report written: $out_file"
