#!/usr/bin/env bash
set -euo pipefail

manifest="${1:-docs/optimization/parity-automation-manifest.tsv}"

if [[ ! -f "$manifest" ]]; then
  echo "Manifest not found: $manifest" >&2
  exit 1
fi

tmp_tests="$(mktemp)"
trap 'rm -f "$tmp_tests"' EXIT

cargo test -q -p yode-tui -- --list >"$tmp_tests"

validate_command() {
  local row_id="$1"
  local command="$2"

  case "$command" in
    "cargo test -p yode-tui "*)
      local pattern
      pattern="${command#cargo test -p yode-tui }"
      pattern="${pattern%% *}"
      if ! grep -Fq "$pattern" "$tmp_tests"; then
        echo "Row $row_id references missing yode-tui test: $pattern" >&2
        exit 1
      fi
      ;;
    "bash "scripts/*)
      local script_path
      script_path="${command#bash }"
      script_path="${script_path%% *}"
      if [[ ! -f "$script_path" ]]; then
        echo "Row $row_id references missing script: $script_path" >&2
        exit 1
      fi
      bash -n "$script_path"
      ;;
    "test -f "*)
      local target
      target="${command#test -f }"
      if [[ ! -f "$target" ]]; then
        echo "Row $row_id references missing file: $target" >&2
        exit 1
      fi
      ;;
    *)
      echo "Row $row_id uses unsupported command form: $command" >&2
      exit 1
      ;;
  esac
}

while IFS=$'\t' read -r row_id _round _category _surface _owner command _evidence; do
  [[ "$row_id" == "id" ]] && continue
  [[ -z "${row_id:-}" ]] && continue
  validate_command "$row_id" "$command"
done <"$manifest"

echo "Parity command audit ok"
