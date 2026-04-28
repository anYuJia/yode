#!/usr/bin/env bash
set -euo pipefail

replay_dir="${1:-.yode/benchmarks/replay}"
index="$replay_dir/replay-index.json"

python3 - "$index" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())
print(f"version={data.get('version')}")
print(f"created_at={data.get('created_at')}")
print(f"fixtures={len(data.get('fixtures', []))}")
for item in data.get("fixtures", [])[:5]:
    print(f"- {item['name']} [{item['kind']}]")
PY
