#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT_DIR="$ROOT_DIR/evals/single_thread"
BENCH_MANIFEST="$SCRIPT_DIR/benchmark_manifest.json"
CORRECTNESS_MANIFEST="$SCRIPT_DIR/correctness_manifest.json"
BENCH_SUITE="gate"
CORRECTNESS_SUITE=""
REPEATS=5
THRESHOLD_PCT=5
CRITICAL_REGRESSION_LIMIT_PCT=-1
BASELINE_ROOT=""
CANDIDATE_ROOT=""
OUT_DIR=""
GOLDEN_DIR=""
DESCRIPTION=""
LOG_TSV=""
SKIP_CORRECTNESS=0

usage() {
  cat <<'EOF'
Usage: benchmark_pair.sh --baseline-root <path> --candidate-root <path> [options]

Options:
  --out-dir <path>
  --manifest <path>
  --suite <name>
  --correctness-manifest <path>
  --correctness-suite <name>
  --golden-dir <path>
  --repeats <count>
  --threshold-pct <pct>
  --critical-regression-limit-pct <pct>
  --description <text>
  --log-tsv <path>
  --skip-correctness
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --baseline-root)
      BASELINE_ROOT="$2"
      shift 2
      ;;
    --candidate-root)
      CANDIDATE_ROOT="$2"
      shift 2
      ;;
    --out-dir)
      OUT_DIR="$2"
      shift 2
      ;;
    --manifest)
      BENCH_MANIFEST="$2"
      shift 2
      ;;
    --suite)
      BENCH_SUITE="$2"
      shift 2
      ;;
    --correctness-manifest)
      CORRECTNESS_MANIFEST="$2"
      shift 2
      ;;
    --correctness-suite)
      CORRECTNESS_SUITE="$2"
      shift 2
      ;;
    --golden-dir)
      GOLDEN_DIR="$2"
      shift 2
      ;;
    --repeats)
      REPEATS="$2"
      shift 2
      ;;
    --threshold-pct)
      THRESHOLD_PCT="$2"
      shift 2
      ;;
    --critical-regression-limit-pct)
      CRITICAL_REGRESSION_LIMIT_PCT="$2"
      shift 2
      ;;
    --description)
      DESCRIPTION="$2"
      shift 2
      ;;
    --log-tsv)
      LOG_TSV="$2"
      shift 2
      ;;
    --skip-correctness)
      SKIP_CORRECTNESS=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown arg: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$BASELINE_ROOT" || -z "$CANDIDATE_ROOT" ]]; then
  usage >&2
  exit 2
fi

if [[ -z "$CORRECTNESS_SUITE" ]]; then
  CORRECTNESS_SUITE="$BENCH_SUITE"
fi

if [[ -z "$OUT_DIR" ]]; then
  OUT_DIR="$ROOT_DIR/evals/artifacts/benchmark/pairs/$(date -u +%Y%m%dT%H%M%SZ)"
fi

mkdir -p "$OUT_DIR/repeats"

CORRECTNESS_STATUS="skipped"
if [[ "$SKIP_CORRECTNESS" -eq 0 ]]; then
  if [[ -z "$GOLDEN_DIR" ]]; then
    echo "--golden-dir is required unless --skip-correctness is set" >&2
    exit 2
  fi
  "$SCRIPT_DIR/run_correctness_suite.sh" \
    verify \
    --root "$CANDIDATE_ROOT" \
    --manifest "$CORRECTNESS_MANIFEST" \
    --suite "$CORRECTNESS_SUITE" \
    --golden-dir "$GOLDEN_DIR" \
    --work-dir "$OUT_DIR/correctness_candidate"
  CORRECTNESS_STATUS="pass"
fi

for repeat in $(seq 1 "$REPEATS"); do
  repeat_dir="$OUT_DIR/repeats/$(printf '%02d' "$repeat")"
  mkdir -p "$repeat_dir"
  if (( repeat % 2 == 1 )); then
    first_label="baseline"
    first_root="$BASELINE_ROOT"
    second_label="candidate"
    second_root="$CANDIDATE_ROOT"
  else
    first_label="candidate"
    first_root="$CANDIDATE_ROOT"
    second_label="baseline"
    second_root="$BASELINE_ROOT"
  fi

  "$SCRIPT_DIR/benchmark_candidate.sh" \
    --root "$first_root" \
    --manifest "$BENCH_MANIFEST" \
    --suite "$BENCH_SUITE" \
    --out "$repeat_dir/${first_label}.json"

  "$SCRIPT_DIR/benchmark_candidate.sh" \
    --root "$second_root" \
    --manifest "$BENCH_MANIFEST" \
    --suite "$BENCH_SUITE" \
    --out "$repeat_dir/${second_label}.json"
done

compare_cmd=(
  "$SCRIPT_DIR/benchmark_compare.py"
  --pair-dir "$OUT_DIR"
  --baseline-root "$BASELINE_ROOT"
  --candidate-root "$CANDIDATE_ROOT"
  --correctness-status "$CORRECTNESS_STATUS"
  --threshold-pct "$THRESHOLD_PCT"
  --critical-regression-limit-pct "$CRITICAL_REGRESSION_LIMIT_PCT"
  --description "$DESCRIPTION"
  --summary-json "$OUT_DIR/summary.json"
)

if [[ -n "$LOG_TSV" ]]; then
  compare_cmd+=(--log-tsv "$LOG_TSV")
fi

"${compare_cmd[@]}"
