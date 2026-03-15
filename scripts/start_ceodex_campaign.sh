#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CAMPAIGN_ID="${1:?usage: start_ceodex_campaign.sh <campaign-id> [extra args...]}"
shift || true

ARTIFACT_ROOT="$ROOT_DIR/evals/artifacts/perf_campaigns/$CAMPAIGN_ID/campaign/ceodex"
LOG_DIR="$ARTIFACT_ROOT/logs"
mkdir -p "$LOG_DIR"

SUPERVISOR_LOG="$LOG_DIR/supervisor.log"
SESSION_NAME="ceodex-$CAMPAIGN_ID"

if tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  echo "session=$SESSION_NAME"
  echo "log=$SUPERVISOR_LOG"
  exit 0
fi

CMD=(
  python3 "$ROOT_DIR/scripts/ceodex_campaign.py"
  --campaign-id "$CAMPAIGN_ID"
  "$@"
)
printf -v CMD_STR '%q ' "${CMD[@]}"
tmux new-session -d -s "$SESSION_NAME" "cd $ROOT_DIR && $CMD_STR >>$SUPERVISOR_LOG 2>&1"

TMUX_PID="$(tmux list-sessions -F '#{session_name} #{session_pid}' | awk -v name="$SESSION_NAME" '$1 == name { print $2 }')"
echo "${TMUX_PID:-}" > "$ARTIFACT_ROOT/launcher.pid"
echo "session=$SESSION_NAME"
echo "pid=${TMUX_PID:-unknown}"
echo "log=$SUPERVISOR_LOG"
