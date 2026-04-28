#!/usr/bin/env bash
set -euo pipefail

replay_dir="${1:-.yode/benchmarks/replay}"

python3 - "$replay_dir" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
index = json.loads((root / "replay-index.json").read_text())
jsonl = [json.loads(line) for line in (root / "replay-index.jsonl").read_text().splitlines() if line.strip()]
items = index.get("fixtures", [])
if len(items) != len(jsonl):
    raise SystemExit("Replay index/jsonl count mismatch")
paths = {(item["name"], item["kind"]): item["path"] for item in items}
for item in jsonl:
    key = (item["name"], item["kind"])
    if paths.get(key) != item["path"]:
        raise SystemExit(f"Replay path mismatch: {key}")
PY

echo "Parity replay json parity ok"
