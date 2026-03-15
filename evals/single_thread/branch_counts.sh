#!/usr/bin/env bash
set -euo pipefail

OUT_DIR="${1:-}"
if [[ -z "$OUT_DIR" ]]; then
  echo "Usage: branch_counts.sh <artifact_dir_with_raw_csvs>" >&2
  exit 2
fi

for scenario in baseline blue2 orange2 white12; do
  csv="$OUT_DIR/raw/analysis_${scenario}.csv"
  if [[ ! -f "$csv" ]]; then
    echo "$scenario: missing ($csv)"
    continue
  fi
  rows="$(wc -l < "$csv" | tr -d '[:space:]')"
  if [[ "$rows" -gt 0 ]]; then
    branches=$((rows - 1))
  else
    branches=0
  fi
  echo "$scenario: branches=$branches"
done
