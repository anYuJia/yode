#!/usr/bin/env bash
set -euo pipefail

replay_dir="${1:-.yode/benchmarks/replay}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

bash scripts/parity-replay-serialize.sh "$tmp_dir/replay" >/dev/null

normalize_json() {
  python3 - "$1" "$2" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())
if isinstance(data, dict):
    data.pop("created_at", None)
    for item in data.get("fixtures", []):
        item.pop("created_at", None)
elif isinstance(data, list):
    for item in data:
        item.pop("created_at", None)
Path(sys.argv[2]).write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")
PY
}

normalize_json "$replay_dir/replay-index.json" "$tmp_dir/current.json"
normalize_json "$tmp_dir/replay/replay-index.json" "$tmp_dir/candidate.json"
diff -u "$tmp_dir/current.json" "$tmp_dir/candidate.json"

echo "Parity replay drift check ok"
