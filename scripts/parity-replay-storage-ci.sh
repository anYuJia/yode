#!/usr/bin/env bash
set -euo pipefail

out_dir="${1:-.yode/benchmarks/replay}"

bash scripts/parity-replay-serialize.sh "$out_dir" >/dev/null

[[ -f "$out_dir/replay-index.json" ]] || { echo "Replay index missing" >&2; exit 1; }
[[ -f "$out_dir/replay-index.jsonl" ]] || { echo "Replay jsonl missing" >&2; exit 1; }

python3 - "$out_dir/replay-index.json" "$out_dir/replay-index.jsonl" <<'PY'
import json
import sys
from pathlib import Path

index = json.loads(Path(sys.argv[1]).read_text())
lines = [json.loads(line) for line in Path(sys.argv[2]).read_text().splitlines() if line.strip()]

if not index.get("fixtures"):
    raise SystemExit("Replay index has no fixtures")
if len(index["fixtures"]) != len(lines):
    raise SystemExit("Replay index and jsonl fixture counts differ")
for item in lines:
    for key in ("name", "kind", "path", "body"):
        if key not in item:
            raise SystemExit(f"Replay fixture missing key: {key}")
PY

echo "Parity replay storage CI ok"
