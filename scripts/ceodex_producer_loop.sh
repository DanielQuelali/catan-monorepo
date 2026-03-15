#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CAMPAIGN_ID="${1:?usage: ceodex_producer_loop.sh <campaign-id> <lane-id> [queue-target] [poll-seconds]}"
LANE_ID="${2:?usage: ceodex_producer_loop.sh <campaign-id> <lane-id> [queue-target] [poll-seconds]}"
QUEUE_TARGET="${3:-2}"
POLL_SECONDS="${4:-20}"
ARTIFACTS_ROOT="$ROOT_DIR/evals/artifacts/perf_campaigns"
AGENT_ID="ceodex-${LANE_ID}"
TOPICS=(
  rollout-loop
  allocation-churn
  winner-aggregation
  action-encoding
  legal-action-hotpath
  resource-hotpath
  state-mutation
  loop-simplification
  branch-elimination
  copy-reduction
)
topic_index=0

while true; do
  if ! active_count="$(
    "$ROOT_DIR/evals/single_thread/perf_campaign.sh" status \
      --campaign-id "$CAMPAIGN_ID" \
      --artifacts-root "$ARTIFACTS_ROOT" \
      --json --show-items | python3 -c 'import json,sys; terminal={"keep","discard","build_fail","correctness_fail","benchmark_fail","policy_fail","stale","cancelled"}; status=json.load(sys.stdin); print(sum(1 for item in status.get("queue_items", []) if item.get("status") not in terminal))'
  )"; then
    echo "status read failed for $AGENT_ID at $(date '+%Y-%m-%d %H:%M:%S'); retrying"
    sleep "$POLL_SECONDS"
    continue
  fi
  if (( active_count >= QUEUE_TARGET )); then
    sleep "$POLL_SECONDS"
    continue
  fi

  topic="${TOPICS[$((topic_index % ${#TOPICS[@]}))]}"
  topic_index=$((topic_index + 1))

  if ! create_output="$(
    "$ROOT_DIR/evals/single_thread/perf_campaign.sh" create-candidate \
      --campaign-id "$CAMPAIGN_ID" \
      --artifacts-root "$ARTIFACTS_ROOT" \
      --agent-id "$AGENT_ID" \
      --topic "$topic" \
      --description "$topic"
  )"; then
    echo "create-candidate failed for $AGENT_ID topic=$topic at $(date '+%Y-%m-%d %H:%M:%S'); retrying"
    sleep "$POLL_SECONDS"
    continue
  fi
  printf '%s\n' "$create_output"

  candidate_id="$(printf '%s\n' "$create_output" | awk -F= '/^candidate_id=/{print $2}')"
  worktree="$(printf '%s\n' "$create_output" | awk -F= '/^worktree=/{print $2}')"
  candidate_dir="$ARTIFACTS_ROOT/$CAMPAIGN_ID/candidates/$candidate_id"
  prompt_file="$candidate_dir/codex_prompt.txt"
  last_message_file="$candidate_dir/codex_last_message.txt"

  cat >"$prompt_file" <<EOF
You are a direct candidate worker for CEOdex on campaign \`$CAMPAIGN_ID\`.

Context:
- Work only in the current git worktree.
- Campaign repo root: \`$ROOT_DIR\`
- Campaign artifacts root: \`$ARTIFACTS_ROOT\`
- Candidate id: \`$candidate_id\`
- Topic hint: \`$topic\`
- Producer lane: \`$AGENT_ID\`

Primary goal:
- Improve single-CPU playout throughput in \`crates/fastcore/src/**\` without changing deterministic results.

Hard constraints:
- Do not edit evaluator files under \`evals/single_thread/**\`.
- Do not edit campaign metadata, queue files, ledger files, or results files directly.
- Do not spawn subagents.
- Do not install dependencies.
- Keep the change simple and localized.

Required workflow:
1. Read enough context from \`program.md\`, \`evals/single_thread/README.md\`, and the hot code you plan to change.
2. Choose one plausible optimization idea related to \`$topic\`.
3. Modify only \`crates/fastcore/src/**\`.
4. Run at least one local sanity check that compiles the touched code. Prefer a targeted \`cargo build\` or \`cargo test\`.
5. Commit exactly one candidate commit with a short message.
6. Submit the candidate with:
   \`$ROOT_DIR/evals/single_thread/perf_campaign.sh submit-candidate --campaign-id $CAMPAIGN_ID --candidate-id $candidate_id --artifacts-root $ARTIFACTS_ROOT --description "<short description>"\`
7. In the final message, state the description you submitted.

Fallback:
- If you inspect the code and decide there is no safe improvement to attempt quickly, leave the worktree clean and end your final message with \`NO_CHANGE\`.
EOF

  codex exec \
    --dangerously-bypass-approvals-and-sandbox \
    --skip-git-repo-check \
    --color never \
    -C "$worktree" \
    --add-dir "$ROOT_DIR" \
    --output-last-message "$last_message_file" \
    - <"$prompt_file" || true

  if ! candidate_info="$(
    python3 - "$candidate_dir/candidate.json" <<'PY'
import json, sys
candidate = json.load(open(sys.argv[1]))
print(candidate.get("status", ""))
print(candidate.get("baseline_commit_snapshot", ""))
PY
  )"; then
    echo "candidate metadata read failed for $candidate_id at $(date '+%Y-%m-%d %H:%M:%S'); retrying"
    sleep "$POLL_SECONDS"
    continue
  fi
  candidate_status="$(printf '%s\n' "$candidate_info" | sed -n '1p')"
  baseline_commit="$(printf '%s\n' "$candidate_info" | sed -n '2p')"

  if [[ "$candidate_status" == "created" || -z "$candidate_status" ]]; then
    if ! head_commit="$(git -C "$worktree" rev-parse HEAD)"; then
      echo "head read failed for $candidate_id at $(date '+%Y-%m-%d %H:%M:%S')"
      sleep "$POLL_SECONDS"
      continue
    fi
    if ! dirty_status="$(git -C "$worktree" status --short)"; then
      echo "status read failed for worktree $worktree at $(date '+%Y-%m-%d %H:%M:%S')"
      sleep "$POLL_SECONDS"
      continue
    fi
    if [[ "$head_commit" != "$baseline_commit" && -z "$dirty_status" ]]; then
      "$ROOT_DIR/evals/single_thread/perf_campaign.sh" submit-candidate \
        --campaign-id "$CAMPAIGN_ID" \
        --artifacts-root "$ARTIFACTS_ROOT" \
        --candidate-id "$candidate_id" \
        --description "$topic" || true
    else
      "$ROOT_DIR/evals/single_thread/perf_campaign.sh" cleanup-candidate \
        --campaign-id "$CAMPAIGN_ID" \
        --artifacts-root "$ARTIFACTS_ROOT" \
        --candidate-id "$candidate_id" \
        --force || true
    fi
  fi
done
