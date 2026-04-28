#!/usr/bin/env bash
set -euo pipefail

manifest="docs/optimization/parity-automation-manifest.tsv"
row_id=""
surface=""

while (( $# > 0 )); do
  case "$1" in
    --row)
      row_id="${2:-}"
      shift 2
      ;;
    --surface)
      surface="${2:-}"
      shift 2
      ;;
    *)
      echo "Usage: $0 [--row <id>] [--surface <surface>]" >&2
      exit 1
      ;;
  esac
done

if [[ -n "$row_id" ]]; then
  line="$(awk -F '\t' -v row_id="$row_id" 'NR > 1 && $1 == row_id { print; exit }' "$manifest")"
  if [[ -z "$line" ]]; then
    echo "Manifest row not found: $row_id" >&2
    exit 1
  fi
  IFS=$'\t' read -r _id round category mapped_surface owner command evidence <<<"$line"
  echo "row=$row_id"
  echo "round=$round"
  echo "category=$category"
  echo "surface=$mapped_surface"
  echo "owner=$owner"
  echo "command=$command"
  echo "evidence=$evidence"
  echo
  bash scripts/parity-owner-route.sh "$mapped_surface"
  exit 0
fi

if [[ -n "$surface" ]]; then
  bash scripts/parity-owner-route.sh "$surface"
  exit 0
fi

echo "Usage: $0 [--row <id>] [--surface <surface>]" >&2
exit 1
