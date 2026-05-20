#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/parity-contract-triage-template.md}"
manifest="${2:-docs/optimization/parity-contract-manifest.tsv}"

[[ -f "$manifest" ]] || { echo "Missing parity contract manifest: $manifest" >&2; exit 1; }
mkdir -p "$(dirname "$out_file")"

{
  echo "# Parity Contract Failure Triage Template"
  echo
  echo "Use this template from uploaded CI parity artifact bundles. Start with the contract that matches the failed job or artifact name, then run the listed focused scripts locally."
  echo
  awk -F '\t' '
    NR == 1 { next }
    {
      fixture_count = split($4, fixtures, ";")
      script_count = split($5, scripts, ";")
      print "## " $1 " (" $2 ")"
      print ""
      print "- owner: " $3
      print "- ci job: " $6
      print "- uploaded artifact: " $7
      print "- triage doc: " $8
      print "- artifact bundle: `.yode/parity-artifacts`"
      print "- manifest: `.yode/parity-artifacts/MANIFEST.md`"
      print ""
      print "Fixtures / docs:"
      for (idx = 1; idx <= fixture_count; idx++) {
        print "- `" fixtures[idx] "`"
      }
      print ""
      print "Focused reruns:"
      for (idx = 1; idx <= script_count; idx++) {
        print "- `bash " scripts[idx] "`"
      }
      print ""
      print "Closeout fields:"
      print "- failed ci run:"
      print "- first failing command:"
      print "- artifact evidence:"
      print "- local rerun result:"
      print "- owner handoff:"
      print ""
    }
  ' "$manifest"
} >"$out_file"

echo "Parity contract triage template written: $out_file"
