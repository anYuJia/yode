#!/usr/bin/env bash
set -euo pipefail

manifest="${1:-docs/optimization/parity-contract-manifest.tsv}"
workflow=".github/workflows/ci.yml"
expected_header=$'contract_id\tcategory\towner\tfixtures\tscripts\tci_job\tartifact\ttriage_doc'
required_categories=$'command-output\nreplay\nvisual\nartifacts\ndocs'

[[ -f "$manifest" ]] || { echo "Missing parity contract manifest: $manifest" >&2; exit 1; }
[[ -f "$workflow" ]] || { echo "Missing CI workflow: $workflow" >&2; exit 1; }

header="$(head -n 1 "$manifest")"
[[ "$header" == "$expected_header" ]] || {
  echo "Unexpected parity contract manifest header" >&2
  echo "Expected: $expected_header" >&2
  echo "Actual:   $header" >&2
  exit 1
}

tmp_categories="$(mktemp)"
tmp_contracts="$(mktemp)"
cleanup() {
  rm -f "$tmp_categories" "$tmp_contracts"
}
trap cleanup EXIT

tail -n +2 "$manifest" | while IFS=$'\t' read -r contract_id category owner fixtures scripts ci_job artifact triage_doc extra; do
  [[ -z "${contract_id}${category}${owner}${fixtures}${scripts}${ci_job}${artifact}${triage_doc}${extra:-}" ]] && continue

  if [[ -n "${extra:-}" ]]; then
    echo "Too many columns in $contract_id" >&2
    exit 1
  fi

  for field_name in contract_id category owner fixtures scripts ci_job artifact triage_doc; do
    value="${!field_name}"
    [[ -n "$value" ]] || { echo "Empty $field_name in $contract_id" >&2; exit 1; }
  done

  [[ "$contract_id" == CONTRACT-* ]] || { echo "Contract id must start with CONTRACT-: $contract_id" >&2; exit 1; }
  printf '%s\n' "$contract_id" >> "$tmp_contracts"
  printf '%s\n' "$category" >> "$tmp_categories"

  IFS=';' read -r -a fixture_items <<< "$fixtures"
  for fixture in "${fixture_items[@]}"; do
    [[ -e "$fixture" ]] || { echo "Missing fixture/doc for $contract_id: $fixture" >&2; exit 1; }
  done

  IFS=';' read -r -a script_items <<< "$scripts"
  for script_path in "${script_items[@]}"; do
    [[ -f "$script_path" ]] || { echo "Missing script for $contract_id: $script_path" >&2; exit 1; }
  done

  rg -q "^  ${ci_job}:" "$workflow" || { echo "Missing CI job for $contract_id: $ci_job" >&2; exit 1; }
  rg -q "$artifact" "$workflow" || { echo "Missing CI artifact for $contract_id: $artifact" >&2; exit 1; }
  [[ -f "$triage_doc" ]] || { echo "Missing triage doc for $contract_id: $triage_doc" >&2; exit 1; }
done

while IFS= read -r category; do
  rg -qx "$category" "$tmp_categories" || { echo "Missing contract category: $category" >&2; exit 1; }
done <<< "$required_categories"

if [[ "$(sort "$tmp_contracts" | uniq -d | wc -l | tr -d ' ')" != "0" ]]; then
  echo "Duplicate contract ids found" >&2
  sort "$tmp_contracts" | uniq -d >&2
  exit 1
fi

echo "Parity contracts CI ok"
