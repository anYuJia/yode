#!/usr/bin/env bash
set -euo pipefail

replay_dir="${1:-.yode/benchmarks/replay}"
out_file="${2:-docs/optimization/275-eighth-replay-jsonl-inventory.md}"

bash scripts/parity-replay-serialize.sh "$replay_dir" >/dev/null

python3 - "$replay_dir/replay-index.jsonl" "$out_file" <<'PY'
import json
import sys
from pathlib import Path

records = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines() if line.strip()]
out = ["# Eighth Replay JSONL Inventory", ""]
for item in records:
    out.append(f"- {item['name']} [{item['kind']}] -> {item['path']}")
Path(sys.argv[2]).write_text("\n".join(out) + "\n")
PY

echo "Parity replay jsonl inventory written: $out_file"
