#!/usr/bin/env bash
set -euo pipefail

keep_ansi=0
keep_hyperlinks=0
cjk_report=0
out_file=""

while (( $# > 0 )); do
  case "$1" in
    --keep-ansi)
      keep_ansi=1
      shift
      ;;
    --keep-hyperlinks)
      keep_hyperlinks=1
      shift
      ;;
    --cjk-width-report)
      cjk_report=1
      shift
      ;;
    --out)
      out_file="${2:-}"
      shift 2
      ;;
    *)
      break
      ;;
  esac
done

baseline="${1:-}"
candidate="${2:-}"

if [[ -z "$baseline" || -z "$candidate" ]]; then
  echo "Usage: $0 [--keep-ansi] [--keep-hyperlinks] [--cjk-width-report] [--out <file>] <baseline> <candidate>" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

normalize() {
  local src="$1"
  local dest="$2"
  cp "$src" "$dest"
  if (( keep_hyperlinks == 0 )); then
    perl -0pi -e 's/\e]8;;[^\a\x1b]*(?:\a|\e\\\\)//g; s/\e]8;;(?:\a|\e\\\\)//g' "$dest"
  fi
  if (( keep_ansi == 0 )); then
    perl -0pi -e 's/\e\[[0-9;?]*[ -\/]*[@-~]//g' "$dest"
  fi
}

norm_base="$tmp_dir/baseline.txt"
norm_candidate="$tmp_dir/candidate.txt"
normalize "$baseline" "$norm_base"
normalize "$candidate" "$norm_candidate"

diff_out="$tmp_dir/diff.txt"
diff_status=0
if ! diff -u "$norm_base" "$norm_candidate" >"$diff_out"; then
  diff_status=$?
fi

cjk_status=0
cjk_out="$tmp_dir/cjk.txt"
if (( cjk_report == 1 )); then
  python3 - "$norm_base" "$norm_candidate" >"$cjk_out" <<'PY'
import sys
import unicodedata
from pathlib import Path

def width(text: str) -> int:
    total = 0
    for ch in text.rstrip("\n"):
        if unicodedata.east_asian_width(ch) in {"W", "F"}:
            total += 2
        else:
            total += 1
    return total

def has_wide(text: str) -> bool:
    return any(unicodedata.east_asian_width(ch) in {"W", "F"} for ch in text)

base = Path(sys.argv[1]).read_text().splitlines()
candidate = Path(sys.argv[2]).read_text().splitlines()
max_len = max(len(base), len(candidate))
mismatches = []
wide_lines = 0
for idx in range(max_len):
    left = base[idx] if idx < len(base) else ""
    right = candidate[idx] if idx < len(candidate) else ""
    if has_wide(left) or has_wide(right):
        wide_lines += 1
        lw = width(left)
        rw = width(right)
        if lw != rw:
            mismatches.append((idx + 1, lw, rw, left, right))

print(f"wide_lines={wide_lines}")
if mismatches:
    print("width_mismatches=yes")
    for line_no, lw, rw, left, right in mismatches:
        print(f"line={line_no} baseline_width={lw} candidate_width={rw}")
        print(f"  baseline={left}")
        print(f"  candidate={right}")
else:
    print("width_mismatches=no")
PY
  if grep -q 'width_mismatches=yes' "$cjk_out"; then
    cjk_status=1
  fi
fi

if [[ -n "$out_file" ]]; then
  {
    cat "$diff_out"
    if (( cjk_report == 1 )); then
      printf '\n## CJK Width Report\n\n'
      cat "$cjk_out"
      printf '\n'
    fi
  } >"$out_file"
fi

if (( diff_status != 0 )); then
  cat "$diff_out"
fi
if (( cjk_report == 1 )); then
  cat "$cjk_out"
fi

if (( diff_status != 0 || cjk_status != 0 )); then
  exit 1
fi

echo "Parity visual diff ok"
