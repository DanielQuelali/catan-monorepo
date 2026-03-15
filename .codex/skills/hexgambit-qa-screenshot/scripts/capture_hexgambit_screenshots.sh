#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
BASE_URL=""
PORT="18081"
STARTUP_DELAY="1.2"
SERVER_PID=""

usage() {
  cat <<'EOF'
Usage: capture_hexgambit_screenshots.sh [--base-url URL] [--port PORT] [--startup-delay SECONDS]

Runs hexgambit-qa/audit.mjs and prints JSON summary with screenshot paths.
- If --base-url is omitted, starts a temporary local server from apps/hex-gambit.
EOF
}

cleanup() {
  if [[ -n "${SERVER_PID}" ]]; then
    kill "${SERVER_PID}" >/dev/null 2>&1 || true
    wait "${SERVER_PID}" 2>/dev/null || true
  fi
}
trap cleanup EXIT

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base-url)
      BASE_URL="${2:-}"
      shift 2
      ;;
    --port)
      PORT="${2:-}"
      shift 2
      ;;
    --startup-delay)
      STARTUP_DELAY="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ ! -d "${REPO_ROOT}/apps/hex-gambit" || ! -d "${REPO_ROOT}/hexgambit-qa" ]]; then
  echo "Repo root is invalid: ${REPO_ROOT}" >&2
  exit 2
fi

if [[ -z "${BASE_URL}" ]]; then
  (
    cd "${REPO_ROOT}/apps/hex-gambit"
    python3 -m http.server "${PORT}" >/dev/null 2>&1
  ) &
  SERVER_PID="$!"
  sleep "${STARTUP_DELAY}"
  BASE_URL="http://127.0.0.1:${PORT}"
fi

REPORT_PATH="$(
  cd "${REPO_ROOT}/hexgambit-qa"
  HEX_GAMBIT_URL="${BASE_URL}" node audit.mjs | tail -n 1
)"

node - "${REPORT_PATH}" <<'NODE'
const fs = require("node:fs");
const path = require("node:path");

const reportPath = process.argv[2];
const report = JSON.parse(fs.readFileSync(reportPath, "utf-8"));
const output = {
  base_url: report.baseUrl,
  report_path: reportPath,
  desktop_intro: report.desktop.screenshots.intro,
  desktop_placement: report.desktop.screenshots.placement,
  mobile_intro: report.mobile.screenshots.intro,
  mobile_placement: report.mobile.screenshots.placement,
  out_dir: path.dirname(reportPath),
};
process.stdout.write(`${JSON.stringify(output, null, 2)}\n`);
NODE
