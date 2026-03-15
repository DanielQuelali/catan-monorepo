#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SUITE="${1:-smoke}"
OUT_DIR="${2:-$ROOT_DIR/evals/artifacts/golden/single_thread/$SUITE}"

STATE_PATH="$ROOT_DIR/data/state_pre_last_settlement.json"
BOARD_PATH="$ROOT_DIR/data/board_example.json"
MAX_TURNS="${MAX_TURNS:-1000}"
START_SEED="${START_SEED:-1000}"
BRANCH_LIMIT="${BRANCH_LIMIT:-}"

case "$SUITE" in
  smoke)
    SEED_COUNT="${SEED_COUNT:-64}"
    if [[ -z "$BRANCH_LIMIT" ]]; then
      # Smoke should complete quickly; full branch fanout can be very large.
      BRANCH_LIMIT=32
    fi
    ;;
  gate)
    SEED_COUNT="${SEED_COUNT:-2048}"
    ;;
  deep)
    SEED_COUNT="${SEED_COUNT:-20000}"
    ;;
  *)
    echo "Unknown suite: $SUITE (expected smoke|gate|deep)" >&2
    exit 2
    ;;
esac

mkdir -p "$OUT_DIR/raw" "$OUT_DIR/normalized"
{
  echo "suite=$SUITE"
  echo "seed_count=$SEED_COUNT"
  echo "start_seed=$START_SEED"
  echo "max_turns=$MAX_TURNS"
  echo "branch_limit=${BRANCH_LIMIT:-full}"
} > "$OUT_DIR/run_config.txt"

echo "Generating fastcore deterministic report -> $OUT_DIR/engine_report.json"
cargo run --release -p fastcore --bin deterministic_regression -- \
  --seed-start "$START_SEED" \
  --seed-count "$SEED_COUNT" \
  --max-turns "$MAX_TURNS" \
  --out "$OUT_DIR/engine_report.json"

run_iba_scenario() {
  local scenario="$1"
  local extra_flag="$2"
  local raw_csv="$OUT_DIR/raw/analysis_${scenario}.csv"
  local norm_csv="$OUT_DIR/normalized/analysis_${scenario}.csv"

  echo "Generating initial-branch-analysis [$scenario] -> $raw_csv"
  local common_args=(
    --state "$STATE_PATH"
    --board "$BOARD_PATH"
    --start-seed "$START_SEED"
    --num-sims "$SEED_COUNT"
    --workers 1
    --max-turns "$MAX_TURNS"
    --output "$raw_csv"
  )
  if [[ -n "$BRANCH_LIMIT" ]]; then
    common_args+=(--limit "$BRANCH_LIMIT")
  fi

  if [[ -n "$extra_flag" ]]; then
    cargo run --release -p initial-branch-analysis -- \
      "${common_args[@]}" \
      "$extra_flag"
  else
    cargo run --release -p initial-branch-analysis -- \
      "${common_args[@]}"
  fi

  "$ROOT_DIR/evals/single_thread/normalize_iba_csv.py" "$raw_csv" "$norm_csv"
}

run_iba_scenario "baseline" ""
run_iba_scenario "blue2" "--blue2"
run_iba_scenario "orange2" "--orange2"
run_iba_scenario "white12" "--white12"

echo "Golden artifacts generated at $OUT_DIR"
