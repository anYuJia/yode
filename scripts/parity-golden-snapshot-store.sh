#!/usr/bin/env bash
set -euo pipefail

source_dir="${1:-.yode/benchmarks}"
target_dir="${2:-.yode/benchmarks/golden/current}"

if [[ ! -d "$source_dir" ]]; then
  echo "Snapshot source directory not found: $source_dir" >&2
  exit 1
fi

rm -rf "$target_dir"
mkdir -p "$target_dir"

copy_if_present() {
  local src="$1"
  local dest="$2"
  if [[ -e "$src" ]]; then
    if [[ -d "$src" ]]; then
      cp -R "$src" "$dest"
    else
      mkdir -p "$(dirname "$dest")"
      cp "$src" "$dest"
    fi
  fi
}

copy_if_present "$source_dir/output-regression-snapshot.md" "$target_dir/output-regression-snapshot.md"
copy_if_present "$source_dir/long-session-benchmark.md" "$target_dir/long-session-benchmark.md"
copy_if_present "$source_dir/output-regression-sections" "$target_dir/output-regression-sections"
copy_if_present "$source_dir/catalogs" "$target_dir/catalogs"

manifest="$target_dir/MANIFEST.md"
{
  echo "# Golden Snapshot Manifest"
  echo
  echo "- source: $source_dir"
  echo "- stored_at: $(date '+%Y-%m-%d %H:%M:%S')"
  echo
  echo "## Files"
  echo
  find "$target_dir" -type f \
    ! -name MANIFEST.md \
    | sort \
    | sed "s#^$target_dir/#- #"
} >"$manifest"

echo "Golden snapshots stored in $target_dir"
