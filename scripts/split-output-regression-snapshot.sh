#!/usr/bin/env bash
set -euo pipefail

snapshot_file="${1:-.yode/benchmarks/output-regression-snapshot.md}"
out_dir="${2:-.yode/benchmarks/output-regression-sections}"

mkdir -p "$out_dir"
rm -f "$out_dir"/*.md

awk -v out_dir="$out_dir" '
  function slugify(text,    slug) {
    slug = tolower(text)
    gsub(/[^a-z0-9]+/, "-", slug)
    gsub(/^-+|-+$/, "", slug)
    if (slug == "") slug = "section"
    return slug
  }
  /^## / {
    section = substr($0, 4)
    file = out_dir "/" slugify(section) ".md"
    print $0 > file
    next
  }
  file != "" { print $0 >> file }
' "$snapshot_file"

echo "Split snapshot sections written to $out_dir"
