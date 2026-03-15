#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CAMPAIGN_ID="${1:?usage: ceodex_cleanup_loop.sh <campaign-id> [interval-seconds]}"
INTERVAL_SECONDS="${2:-120}"
ARTIFACTS_ROOT="$ROOT_DIR/evals/artifacts/perf_campaigns"
CANDIDATES_DIR="$ARTIFACTS_ROOT/$CAMPAIGN_ID/candidates"

while true; do
  if [[ -d "$CANDIDATES_DIR" ]]; then
    while IFS= read -r -d '' candidate_json; do
      values="$(
        python3 - "$candidate_json" <<'PY'
import json, sys
candidate = json.load(open(sys.argv[1]))
print(candidate.get("candidate_id", ""))
print(candidate.get("status", ""))
print("1" if candidate.get("worktree_removed") else "0")
PY
      )"
      candidate_id="$(printf '%s\n' "$values" | sed -n '1p')"
      status="$(printf '%s\n' "$values" | sed -n '2p')"
      removed="$(printf '%s\n' "$values" | sed -n '3p')"
      case "$status" in
        keep|discard|build_fail|correctness_fail|benchmark_fail|policy_fail|stale|cancelled|accepted)
          if [[ "$removed" != "1" && -n "$candidate_id" ]]; then
            "$ROOT_DIR/evals/single_thread/perf_campaign.sh" cleanup-candidate \
              --campaign-id "$CAMPAIGN_ID" \
              --artifacts-root "$ARTIFACTS_ROOT" \
              --candidate-id "$candidate_id" || true
          fi
          ;;
      esac
    done < <(find "$CANDIDATES_DIR" -name candidate.json -print0)
  fi
  sleep "$INTERVAL_SECONDS"
done
