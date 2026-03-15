#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

exec "$ROOT_DIR/evals/single_thread/correctness_runner.py" "$@"
