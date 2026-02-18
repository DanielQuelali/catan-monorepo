#!/usr/bin/env bash
set -euo pipefail

seeds="${1:-1,2,3}"
max_turns="${2:-}"

if [[ -n "${max_turns}" ]]; then
  while true; do
    cargo run --quiet --bin smoke -- --seeds "${seeds}" --max-turns "${max_turns}"
    sleep 0.2
  done
else
  while true; do
    cargo run --quiet --bin smoke -- --seeds "${seeds}"
    sleep 0.2
  done
fi
