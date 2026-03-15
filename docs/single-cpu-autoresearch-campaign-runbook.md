# Single-CPU Autoresearch Campaign Runbook

Date: 2026-03-14

This runbook defines the operational path for the single-CPU performance campaign controller at:

- `evals/single_thread/perf_campaign.sh`

The objective is unchanged:

- maximize throughput
- preserve deterministic outputs
- keep one immutable evaluator

## Prerequisites

- Run from repo root.
- Ensure `cargo` build dependencies are available.
- Use one benchmark host/lane for production benchmark decisions.
- Keep campaign evaluator files immutable during candidate optimization.

## Immutable Evaluator Files

Treat the following as frozen for an active campaign:

- `evals/single_thread/generate_golden.sh`
- `evals/single_thread/verify_against_golden.sh`
- `evals/single_thread/correctness_manifest.json`
- `evals/single_thread/run_correctness_suite.sh`
- `evals/single_thread/benchmark_manifest.json`
- `evals/single_thread/benchmark_candidate.sh`
- `evals/single_thread/benchmark_pair.sh`
- `evals/single_thread/benchmark_compare.py`

## Campaign Artifact Layout

Each campaign writes under:

- `evals/artifacts/perf_campaigns/<campaign-id>/campaign/`
- `evals/artifacts/perf_campaigns/<campaign-id>/campaign/frozen/`
- `evals/artifacts/perf_campaigns/<campaign-id>/goldens/`
- `evals/artifacts/perf_campaigns/<campaign-id>/queue/`
- `evals/artifacts/perf_campaigns/<campaign-id>/ledger/`
- `evals/artifacts/perf_campaigns/<campaign-id>/candidates/<candidate-id>/`
- `evals/artifacts/perf_campaigns/<campaign-id>/baseline/`

## Phase 1: Initialize Campaign

Initialize once per campaign.

```bash
evals/single_thread/perf_campaign.sh init \
  --campaign-id gate-20260314 \
  --baseline-ref HEAD \
  --benchmark-suite gate \
  --correctness-suite gate \
  --threshold-pct 2 \
  --critical-regression-limit-pct -1 \
  --benchmark-repeats 5 \
  --generate-goldens
```

What this does:

- freezes manifests into `campaign/frozen/`
- records baseline branch, commit, and worktree
- creates queue and ledger locations
- optionally generates canonical correctness goldens

## Phase 2: Create Candidate Worktrees

Create one worktree per candidate attempt.

```bash
evals/single_thread/perf_campaign.sh create-candidate \
  --campaign-id gate-20260314 \
  --agent-id worker-01 \
  --topic rollout-loop-tighten
```

Output includes:

- `candidate_id`
- `candidate_branch`
- `candidate_worktree`
- baseline commit snapshot

Rules:

- branch from current baseline commit only
- keep candidate worktree clean before submission
- commit candidate changes before submission

## Phase 3: Submit Candidate

Submit after candidate commits are ready.

```bash
evals/single_thread/perf_campaign.sh submit-candidate \
  --campaign-id gate-20260314 \
  --candidate-id <candidate-id>
```

Preflight checks:

- candidate worktree branch matches registered branch
- candidate worktree is clean
- immutable evaluator files were not changed
- candidate is based on current baseline
- changed files are inside mutable performance surface by default

Override for out-of-surface changes:

```bash
evals/single_thread/perf_campaign.sh submit-candidate \
  --campaign-id gate-20260314 \
  --candidate-id <candidate-id> \
  --allow-outside-mutable-surface \
  --outside-mutable-justification "requires shared utility change"
```

## Phase 4: Run Serialized Benchmark Worker

Run one benchmark worker on the benchmark host.

Process a single candidate:

```bash
evals/single_thread/perf_campaign.sh worker-run --campaign-id gate-20260314
```

Drain queue:

```bash
evals/single_thread/perf_campaign.sh worker-run --campaign-id gate-20260314 --drain
```

Run as a long-lived unattended worker:

```bash
evals/single_thread/perf_campaign.sh worker-run \
  --campaign-id gate-20260314 \
  --drain \
  --continuous \
  --poll-seconds 15
```

Optional automatic baseline advancement on `keep`:

```bash
evals/single_thread/perf_campaign.sh worker-run \
  --campaign-id gate-20260314 \
  --drain \
  --auto-advance
```

Worker behavior:

- enforces one-at-a-time benchmark lane lock
- enforces benchmark host affinity from campaign metadata
- recovers interrupted `running_*` items
- runs correctness first
- runs paired benchmark only after correctness pass
- writes terminal decision and metrics into ledger

Emergency host override:

```bash
evals/single_thread/perf_campaign.sh worker-run \
  --campaign-id gate-20260314 \
  --drain \
  --allow-host-mismatch
```

## Queue States

Queue item states:

- `queued`
- `running_correctness`
- `correctness_fail`
- `queued_for_benchmark`
- `running_benchmark`
- `keep`
- `discard`
- `build_fail`
- `benchmark_fail`
- `policy_fail`
- `stale`
- `cancelled`

`stale` policy:

- if baseline advances, not-yet-finalized items from old baseline are marked stale
- stale items are not benchmarked automatically

## Phase 5: Advance Baseline

Advance baseline only for `keep` candidates.

```bash
evals/single_thread/perf_campaign.sh advance-baseline \
  --campaign-id gate-20260314 \
  --candidate-id <candidate-id> \
  --reason "median speedup above threshold with no critical regression"
```

Baseline advancement behavior:

- requires fast-forward ancestry from baseline to candidate
- updates campaign metadata baseline commit
- appends baseline advancement record
- marks in-flight older-baseline items as stale

## Phase 6: Cleanup

Remove candidate worktree after terminal outcome.

```bash
evals/single_thread/perf_campaign.sh cleanup-candidate \
  --campaign-id gate-20260314 \
  --candidate-id <candidate-id>
```

Cancel candidate without benchmarking:

```bash
evals/single_thread/perf_campaign.sh cancel-candidate \
  --campaign-id gate-20260314 \
  --candidate-id <candidate-id> \
  --reason "operator stop"
```

## Status and Inspection

Human-readable status:

```bash
evals/single_thread/perf_campaign.sh status --campaign-id gate-20260314
```

Machine-readable status:

```bash
evals/single_thread/perf_campaign.sh status --campaign-id gate-20260314 --json --show-items
```

Canonical ledger:

- `evals/artifacts/perf_campaigns/<campaign-id>/ledger/experiments.jsonl`

Baseline history:

- `evals/artifacts/perf_campaigns/<campaign-id>/campaign/baseline_history.jsonl`

## Failure Handling

If a candidate fails correctness:

- queue state becomes `correctness_fail`
- candidate status is terminal
- ledger row is appended

If benchmark execution fails:

- queue state becomes `benchmark_fail`
- candidate status is terminal
- ledger row is appended

If policy/preflight fails:

- queue state becomes `policy_fail` or `stale`
- candidate is not benchmarked
- ledger row is appended

## Recommended Operating Discipline

- Do not run multiple benchmark workers on the same campaign.
- Run candidate generation and lightweight checks in parallel.
- Keep final benchmark decisions serialized on one host.
- Treat campaign artifacts and ledger as the source of truth, not terminal history.
# Superseded

This runbook reflects an older evaluator design that included `initial-branch-analysis`.
The active harness under `evals/single_thread/` is now playout-only (`fastcore` correctness plus `bench_value_state` benchmark).
