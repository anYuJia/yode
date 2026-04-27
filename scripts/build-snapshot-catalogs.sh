#!/usr/bin/env bash
set -euo pipefail

snapshot_file="${1:-.yode/benchmarks/output-regression-snapshot.md}"
out_dir="${2:-.yode/benchmarks/catalogs}"

mkdir -p "$out_dir"

if [ ! -f "$snapshot_file" ]; then
  echo "Snapshot not found: $snapshot_file" >&2
  exit 1
fi

mtime_epoch="$(stat -f %m "$snapshot_file" 2>/dev/null || stat -c %Y "$snapshot_file")"
now_epoch="$(date +%s)"
age_minutes="$(( (now_epoch - mtime_epoch) / 60 ))"
if [ "$age_minutes" -le 10 ]; then
  freshness="fresh"
elif [ "$age_minutes" -le 60 ]; then
  freshness="recent"
else
  freshness="stale"
fi

write_catalog() {
  local title="$1"
  local out_file="$2"
  shift 2
  {
    echo "# $title"
    echo
    echo "- freshness: $freshness"
    echo "- source: $snapshot_file"
    echo
    echo "## Sections"
    echo
    for section in "$@"; do
      echo "- $section"
    done
  } > "$out_file"
}

write_catalog \
  "Inspector Snapshot Catalog" \
  "$out_dir/inspector-snapshot-catalog.md" \
  "Inspector Regression Snapshot > Assistant" \
  "Inspector Regression Snapshot > Tool" \
  "Inspector Regression Snapshot > System" \
  "Inspector Regression Snapshot > Error"

write_catalog \
  "Export Snapshot Catalog" \
  "$out_dir/export-snapshot-catalog.md" \
  "Export Regression Snapshot > Workspace Index" \
  "Export Regression Snapshot > Bundle Completion" \
  "Export Regression Snapshot > Transcript Summary"

write_catalog \
  "Remote Snapshot Catalog" \
  "$out_dir/remote-snapshot-catalog.md" \
  "Remote Bundle Regression Snapshot" \
  "Hook Regression Snapshot"

write_catalog \
  "Transcript Snapshot Catalog" \
  "$out_dir/transcript-snapshot-catalog.md" \
  "Output Regression Snapshot > Assistant Narrow" \
  "Output Regression Snapshot > Grouped Tool Narrow" \
  "Output Regression Snapshot > System Batch Narrow" \
  "Output Regression Snapshot > Turn Status"

echo "Snapshot catalogs written to $out_dir"
