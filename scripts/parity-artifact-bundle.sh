#!/usr/bin/env bash
set -euo pipefail

out_dir="${1:-.yode/parity-artifacts}"
bench_dir="${2:-.yode/benchmarks}"
docs_dir="${3:-docs/optimization}"

rm -rf "$out_dir"
mkdir -p "$out_dir"
mkdir -p "$bench_dir"

bash scripts/parity-contract-triage-template.sh "$bench_dir/failure-triage-template.md" >/dev/null

copy_if_present() {
  local src="$1"
  local dest="$2"
  if [[ -e "$src" ]]; then
    mkdir -p "$(dirname "$dest")"
    if [[ -d "$src" ]]; then
      mkdir -p "$dest"
      cp -R "$src"/. "$dest"/
    else
      cp "$src" "$dest"
    fi
  fi
}

copy_if_present "$bench_dir/output-regression-snapshot.md" "$out_dir/benchmarks/output-regression-snapshot.md"
copy_if_present "$bench_dir/long-session-benchmark.md" "$out_dir/benchmarks/long-session-benchmark.md"
copy_if_present "$bench_dir/output-regression-sections" "$out_dir/benchmarks/output-regression-sections"
copy_if_present "$bench_dir/catalogs" "$out_dir/benchmarks/catalogs"
copy_if_present "$bench_dir/visual-diff-report.md" "$out_dir/benchmarks/visual-diff-report.md"
copy_if_present "$bench_dir/visual-width-report.md" "$out_dir/benchmarks/visual-width-report.md"
copy_if_present "$bench_dir/candidate-compare-report.md" "$out_dir/benchmarks/candidate-compare-report.md"
copy_if_present "$bench_dir/catalog-compare-report.md" "$out_dir/benchmarks/catalog-compare-report.md"
copy_if_present "$bench_dir/failure-route-report.md" "$out_dir/benchmarks/failure-route-report.md"
copy_if_present "$bench_dir/failure-triage-template.md" "$out_dir/benchmarks/failure-triage-template.md"
copy_if_present "$bench_dir/golden/current" "$out_dir/benchmarks/golden-current"
copy_if_present "$bench_dir/replay" "$out_dir/benchmarks/replay"

for doc in \
  243-parity-risk-register.md \
  244-parity-known-limitations.md \
  245-parity-release-note-draft.md \
  255-sixth-parity-handoff.md \
  258-parity-fixture-inventory.md \
  259-sixth-parity-summary-report.md \
  260-sixth-parity-signoff.md \
  261-parity-visual-inventory.md \
  264-eighth-artifact-upload-policy.md \
  269-eighth-failure-report-template.md \
  parity-contract-triage-template.md \
  270-eighth-stored-artifact-closeout.md \
  302-four-month-final-gap-report.md \
  303-release-config-compat-audit.md \
  304-four-month-release-note-draft.md \
  305-release-benchmark-evidence.md
do
  copy_if_present "$docs_dir/$doc" "$out_dir/docs/$doc"
done

manifest="$out_dir/MANIFEST.md"
{
  echo "# Parity Artifact Bundle"
  echo
  echo "- created_at: $(date '+%Y-%m-%d %H:%M:%S')"
  echo
  echo "## Files"
  echo
  find "$out_dir" -type f \
    ! -name MANIFEST.md \
    | sort \
    | sed "s#^$out_dir/#- #"
} >"$manifest"

echo "Parity artifact bundle written: $out_dir"
