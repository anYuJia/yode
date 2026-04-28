#!/usr/bin/env bash
set -euo pipefail

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

src="$tmp_dir/src.md"
dest="$tmp_dir/dest.md"
printf 'a  \n\n\nb\n' >"$src"
bash scripts/parity-replay-normalize.sh "$src" "$dest" >/dev/null

python3 - "$dest" <<'PY'
import sys
from pathlib import Path
text = Path(sys.argv[1]).read_text()
if "\n\n\n" in text:
    raise SystemExit("Replay normalization kept triple blank lines")
PY

echo "Parity replay normalization CI ok"
