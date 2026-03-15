#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CAMPAIGN_ID="${1:?usage: ceodex_worker_loop.sh <campaign-id> [poll-seconds]}"
POLL_SECONDS="${2:-15}"
ARTIFACTS_ROOT="$ROOT_DIR/evals/artifacts/perf_campaigns"

while true; do
  if "$ROOT_DIR/evals/single_thread/perf_campaign.sh" worker-run \
    --campaign-id "$CAMPAIGN_ID" \
    --artifacts-root "$ARTIFACTS_ROOT" \
    --drain \
    --continuous \
    --auto-advance \
    --poll-seconds "$POLL_SECONDS" \
    --worker-id "ceodex-worker"; then
    echo "worker loop exited cleanly at $(date '+%Y-%m-%d %H:%M:%S')"
  else
    echo "worker loop command failed at $(date '+%Y-%m-%d %H:%M:%S'); retrying"
  fi
  sleep 5
done
