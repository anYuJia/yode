#!/usr/bin/env bash
set -euo pipefail

baseline_dir="${1:-.yode/benchmarks/catalogs}"
out_file="${2:-.yode/benchmarks/catalog-compare-report.md}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

bash scripts/output-regression-snapshot.sh "$tmp_dir" >/dev/null
bash scripts/build-snapshot-catalogs.sh \
  "$tmp_dir/output-regression-snapshot.md" \
  "$tmp_dir/catalogs" >/dev/null

normalize_dir() {
  local src_dir="$1"
  local dest_dir="$2"
  mkdir -p "$dest_dir"
  find "$src_dir" -type f | while read -r path; do
    rel="${path#$src_dir/}"
    mkdir -p "$(dirname "$dest_dir/$rel")"
    sed \
      -e 's#^- freshness: .*#- freshness: <normalized>#' \
      -e 's#^- source: .*#- source: <normalized>#' \
      "$path" >"$dest_dir/$rel"
  done
}

normalize_dir "$baseline_dir" "$tmp_dir/baseline-norm"
normalize_dir "$tmp_dir/catalogs" "$tmp_dir/candidate-norm"

if diff -ru "$tmp_dir/baseline-norm" "$tmp_dir/candidate-norm" >"$out_file"; then
  printf 'Parity catalog compare ok\n' >"$out_file"
fi

echo "Parity catalog compare report written: $out_file"
