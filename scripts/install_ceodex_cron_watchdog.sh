#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CAMPAIGN_ID="${1:?usage: install_ceodex_cron_watchdog.sh <campaign-id>}"
CRON_TAG="# ceodex-watchdog:$CAMPAIGN_ID"
CRON_LOG="$ROOT_DIR/evals/artifacts/perf_campaigns/$CAMPAIGN_ID/campaign/ceodex/logs/cron_install.log"
CRON_LINE="* * * * * cd $ROOT_DIR && /bin/bash $ROOT_DIR/scripts/ceodex_cron_watchdog.sh $CAMPAIGN_ID $CRON_TAG"

mkdir -p "$(dirname "$CRON_LOG")"

TMP_CRON="$(mktemp)"
if crontab -l >/dev/null 2>&1; then
  crontab -l | grep -Fv "$CRON_TAG" >"$TMP_CRON" || true
else
  : >"$TMP_CRON"
fi
echo "$CRON_LINE" >>"$TMP_CRON"
crontab "$TMP_CRON"
rm -f "$TMP_CRON"

echo "[$(date '+%Y-%m-%d %H:%M:%S %z')] installed $CRON_TAG" >>"$CRON_LOG"
echo "cron_tag=$CRON_TAG"
echo "cron_line=$CRON_LINE"
