#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$ROOT_DIR/apps/hex-gambit"
PORT="${HEX_GAMBIT_PORT:-8080}"

if [[ ! -d "$APP_DIR" ]]; then
  echo "Hex Gambit app directory not found: $APP_DIR" >&2
  exit 1
fi

if command -v lsof >/dev/null 2>&1; then
  PIDS="$(lsof -ti tcp:"$PORT" || true)"
  if [[ -n "$PIDS" ]]; then
    echo "Stopping process(es) on port $PORT: $PIDS"
    kill -9 $PIDS
  fi
else
  echo "Warning: lsof not found; cannot auto-kill existing process on port $PORT."
fi

echo "Starting Hex Gambit on http://localhost:$PORT"
cd "$APP_DIR"
exec python3 -m http.server "$PORT"
