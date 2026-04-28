#!/usr/bin/env bash
set -euo pipefail

replay_dir="${1:-.yode/benchmarks/replay}"

bash scripts/parity-replay-serialize.sh "$replay_dir" >/dev/null

python3 - "$replay_dir/replay-index.json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())
fixtures = data.get("fixtures", [])
if not fixtures:
    raise SystemExit("Replay fixtures missing")

seen = set()
for item in fixtures:
    key = (item.get("name"), item.get("kind"), item.get("path"))
    if key in seen:
        raise SystemExit(f"Duplicate replay fixture: {key}")
    seen.add(key)
    if not item.get("body", "").strip():
        raise SystemExit(f"Replay fixture body empty: {key}")
PY

echo "Parity replay index CI ok"
