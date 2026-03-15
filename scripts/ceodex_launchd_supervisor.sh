#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CAMPAIGN_ID="${1:?usage: ceodex_launchd_supervisor.sh <campaign-id> [producer-count] [queue-target] [worker-poll] [producer-poll] [cleanup-interval]}"
PRODUCER_COUNT="${2:-2}"
QUEUE_TARGET="${3:-2}"
WORKER_POLL="${4:-15}"
PRODUCER_POLL="${5:-20}"
CLEANUP_INTERVAL="${6:-120}"

ARTIFACT_ROOT="$ROOT_DIR/evals/artifacts/perf_campaigns/$CAMPAIGN_ID/campaign/ceodex"
LOG_DIR="$ARTIFACT_ROOT/logs"
PID_DIR="$ARTIFACT_ROOT/pids"
mkdir -p "$LOG_DIR" "$PID_DIR"

chmod +x \
  "$ROOT_DIR/scripts/ceodex_worker_loop.sh" \
  "$ROOT_DIR/scripts/ceodex_cleanup_loop.sh" \
  "$ROOT_DIR/scripts/ceodex_producer_loop.sh"

supervise_loop() {
  local name="$1"
  local log_file="$2"
  shift 2
  while true; do
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] start $name" >>"$log_file"
    "$@" >>"$log_file" 2>&1 || true
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] restart $name after exit" >>"$log_file"
    sleep 5
  done
}

cleanup() {
  jobs -p | while read -r pid; do
    kill "$pid" 2>/dev/null || true
  done
}

trap cleanup EXIT INT TERM

supervise_loop "worker" "$LOG_DIR/worker.log" \
  "$ROOT_DIR/scripts/ceodex_worker_loop.sh" "$CAMPAIGN_ID" "$WORKER_POLL" &
echo "$!" > "$PID_DIR/worker-supervisor.pid"

supervise_loop "cleanup" "$LOG_DIR/cleanup.log" \
  "$ROOT_DIR/scripts/ceodex_cleanup_loop.sh" "$CAMPAIGN_ID" "$CLEANUP_INTERVAL" &
echo "$!" > "$PID_DIR/cleanup-supervisor.pid"

for lane in $(seq 1 "$PRODUCER_COUNT"); do
  lane_name="$(printf '%02d' "$lane")"
  supervise_loop "producer-$lane_name" "$LOG_DIR/producer-$lane_name.log" \
    "$ROOT_DIR/scripts/ceodex_producer_loop.sh" "$CAMPAIGN_ID" "$lane_name" "$QUEUE_TARGET" "$PRODUCER_POLL" &
  echo "$!" > "$PID_DIR/producer-$lane_name-supervisor.pid"
done

wait
