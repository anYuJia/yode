#!/usr/bin/env bash
set -euo pipefail

out_dir="${1:-.yode/benchmarks/replay}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

mkdir -p "$out_dir"
bash scripts/parity-fixture-pack.sh "$tmp_dir/fixtures" >/dev/null

python3 - "$tmp_dir/fixtures" "$out_dir" <<'PY'
import json
import sys
from pathlib import Path
from datetime import datetime, timezone

def normalize(text: str) -> str:
    lines = [line.rstrip() for line in text.splitlines()]
    out = []
    blank = 0
    for line in lines:
        if line == "":
            blank += 1
            if blank > 1:
                continue
        else:
            blank = 0
        out.append(line)
    return "\n".join(out).rstrip() + "\n"

fixture_dir = Path(sys.argv[1])
out_dir = Path(sys.argv[2])
out_dir.mkdir(parents=True, exist_ok=True)

records = []
version = "v1"
created_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
for path in sorted(fixture_dir.rglob("*")):
    if not path.is_file():
        continue
    body = normalize(path.read_text())
    stem = path.stem
    suffixes = path.suffixes
    kind = suffixes[-2].lstrip(".") if len(suffixes) >= 2 else path.suffix.lstrip(".")
    record = {
        "version": version,
        "created_at": created_at,
        "source_generator": "parity-fixture-pack.sh",
        "name": stem.split(".")[0],
        "kind": kind,
        "path": str(path.relative_to(fixture_dir)),
        "body": body,
    }
    records.append(record)
    single_path = out_dir / f"{record['name']}.{record['kind']}.json"
    single_path.write_text(json.dumps(record, indent=2, ensure_ascii=False) + "\n")

(out_dir / "replay-index.json").write_text(
    json.dumps({"version": version, "created_at": created_at, "fixtures": records}, indent=2, ensure_ascii=False) + "\n"
)

with (out_dir / "replay-index.jsonl").open("w", encoding="utf-8") as fh:
    for record in records:
        fh.write(json.dumps(record, ensure_ascii=False) + "\n")
PY

echo "Parity replay fixtures written: $out_dir"
