#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SUITE="${1:-smoke}"
OUT_DIR="${2:-$ROOT_DIR/evals/artifacts/golden/single_thread/$SUITE}"

START_SEED="${START_SEED:-1000}"
MAX_TURNS="${MAX_TURNS:-1000}"

case "$SUITE" in
  smoke)
    SEED_COUNT="${SEED_COUNT:-64}"
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

mkdir -p "$OUT_DIR"
{
  echo "suite=$SUITE"
  echo "seed_start=$START_SEED"
  echo "seed_count=$SEED_COUNT"
  echo "max_turns=$MAX_TURNS"
} > "$OUT_DIR/run_config.txt"

echo "Generating fastcore deterministic report -> $OUT_DIR/engine_report.json"
cargo run --release -p fastcore --bin deterministic_regression -- \
  --seed-start "$START_SEED" \
  --seed-count "$SEED_COUNT" \
  --max-turns "$MAX_TURNS" \
  --out "$OUT_DIR/engine_report.json"

echo "Golden artifacts generated at $OUT_DIR"
