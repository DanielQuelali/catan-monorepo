#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SUITE="${1:-smoke}"
GOLDEN_DIR="${2:-$ROOT_DIR/evals/artifacts/golden/single_thread/$SUITE}"
WORK_DIR="${3:-$ROOT_DIR/evals/artifacts/verify/single_thread/$SUITE}"

if [[ ! -d "$GOLDEN_DIR" ]]; then
  echo "Golden directory not found: $GOLDEN_DIR" >&2
  exit 2
fi

rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR"

echo "Generating candidate artifacts in $WORK_DIR"
"$ROOT_DIR/evals/single_thread/generate_golden.sh" "$SUITE" "$WORK_DIR"

echo "Comparing engine report"
diff -u "$GOLDEN_DIR/engine_report.json" "$WORK_DIR/engine_report.json"
echo "Comparing run config"
diff -u "$GOLDEN_DIR/run_config.txt" "$WORK_DIR/run_config.txt"

for scenario in baseline blue2 orange2 white12; do
  echo "Comparing normalized CSV [$scenario]"
  diff -u \
    "$GOLDEN_DIR/normalized/analysis_${scenario}.csv" \
    "$WORK_DIR/normalized/analysis_${scenario}.csv"
done

echo "Verification passed for suite $SUITE"
