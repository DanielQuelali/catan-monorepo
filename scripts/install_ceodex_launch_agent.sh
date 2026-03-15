#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CAMPAIGN_ID="${1:?usage: install_ceodex_launch_agent.sh <campaign-id> [producer-count] [queue-target] [worker-poll] [producer-poll] [cleanup-interval]}"
PRODUCER_COUNT="${2:-2}"
QUEUE_TARGET="${3:-2}"
WORKER_POLL="${4:-15}"
PRODUCER_POLL="${5:-20}"
CLEANUP_INTERVAL="${6:-120}"
UID_VALUE="$(id -u)"
LABEL="com.ceodex.${CAMPAIGN_ID}"
LOG_DIR="$ROOT_DIR/evals/artifacts/perf_campaigns/$CAMPAIGN_ID/campaign/ceodex/logs"
LAUNCHD_DIR="$ROOT_DIR/evals/artifacts/perf_campaigns/$CAMPAIGN_ID/campaign/ceodex/launchd"
PLIST_PATH="$LAUNCHD_DIR/$LABEL.plist"
mkdir -p "$LAUNCHD_DIR" "$LOG_DIR"

chmod +x \
  "$ROOT_DIR/scripts/ceodex_launchd_supervisor.sh" \
  "$ROOT_DIR/scripts/ceodex_worker_loop.sh" \
  "$ROOT_DIR/scripts/ceodex_cleanup_loop.sh" \
  "$ROOT_DIR/scripts/ceodex_producer_loop.sh"

cat >"$PLIST_PATH" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>$LABEL</string>
  <key>ProgramArguments</key>
  <array>
    <string>$ROOT_DIR/scripts/ceodex_launchd_supervisor.sh</string>
    <string>$CAMPAIGN_ID</string>
    <string>$PRODUCER_COUNT</string>
    <string>$QUEUE_TARGET</string>
    <string>$WORKER_POLL</string>
    <string>$PRODUCER_POLL</string>
    <string>$CLEANUP_INTERVAL</string>
  </array>
  <key>WorkingDirectory</key>
  <string>$ROOT_DIR</string>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>$LOG_DIR/launchd.out.log</string>
  <key>StandardErrorPath</key>
  <string>$LOG_DIR/launchd.err.log</string>
</dict>
</plist>
EOF

launchctl bootout "gui/$UID_VALUE/$LABEL" >/dev/null 2>&1 || true
launchctl bootstrap "gui/$UID_VALUE" "$PLIST_PATH"
launchctl enable "gui/$UID_VALUE/$LABEL" >/dev/null 2>&1 || true
launchctl kickstart -k "gui/$UID_VALUE/$LABEL"

echo "label=$LABEL"
echo "plist=$PLIST_PATH"
echo "stdout_log=$LOG_DIR/launchd.out.log"
echo "stderr_log=$LOG_DIR/launchd.err.log"
