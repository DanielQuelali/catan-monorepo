#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CAMPAIGN_ID="${1:?usage: ceodex_cron_watchdog.sh <campaign-id>}"
CEO_ROOT="$ROOT_DIR/evals/artifacts/perf_campaigns/$CAMPAIGN_ID/campaign/ceodex"
LOG_DIR="$CEO_ROOT/logs"
STATE_FILE="$CEO_ROOT/restart_state.json"
PID_FILE="$CEO_ROOT/supervisor.pid"
WATCHDOG_LOG="$LOG_DIR/cron_watchdog.log"
SESSION_NAME="ceodex-$CAMPAIGN_ID"

mkdir -p "$LOG_DIR"

is_running() {
  if [[ -f "$PID_FILE" ]]; then
    local pid
    pid="$(tr -d '[:space:]' <"$PID_FILE")"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      return 0
    fi
  fi
  if tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
    return 0
  fi
  return 1
}

if is_running; then
  exit 0
fi

if [[ -f "$STATE_FILE" ]]; then
  RESTART_AFTER="$(python3 - "$STATE_FILE" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
try:
    payload = json.loads(path.read_text())
except Exception:
    print("")
    raise SystemExit(0)
value = payload.get("restart_after")
print("" if value is None else int(value))
PY
)"
  NOW_EPOCH="$(date +%s)"
  if [[ -n "$RESTART_AFTER" ]] && (( NOW_EPOCH < RESTART_AFTER )); then
    exit 0
  fi
fi

echo "[$(date '+%Y-%m-%d %H:%M:%S %z')] starting CEOdex" >>"$WATCHDOG_LOG"
"$ROOT_DIR/scripts/start_ceodex_campaign.sh" "$CAMPAIGN_ID" >>"$WATCHDOG_LOG" 2>&1
rm -f "$STATE_FILE"
