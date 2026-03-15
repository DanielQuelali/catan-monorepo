# Single-CPU Throughput Autoresearch Plan

Date: 2026-03-14

## Goal

Increase single-CPU playout throughput as much as possible without changing results.

This plan is for a swarm of agents that will create git worktrees, generate candidate optimizations, and evaluate them automatically.

## Executive Summary

Do not use the current regression harness as the optimization objective by itself.

Use a two-stage evaluator:

1. `Correctness gate`
   - Exact deterministic outputs must match the baseline.
   - This uses the existing single-thread regression harness plus expanded workload coverage.

2. `Performance gate`
   - Candidates that pass correctness enter a dedicated serialized benchmark lane.
   - Benchmark with `workers=1`, fixed workloads, repeated paired runs, and median paired improvement.

Parallelize candidate generation and correctness checks. Do not parallelize final performance measurement on the same benchmark lane.

## Key Lessons From `autoresearch`

The `autoresearch` repo is effective because it has a very strict separation between:

- immutable evaluation code
- mutable optimization surface
- one scalar objective
- a simple keep/discard loop

What matters from `/tmp/autoresearch`:

- `README.md`
  - The agent edits one file and is judged by a fixed evaluator.
  - The objective is a single scalar metric.
  - Training runs on a fixed budget.
- `prepare.py`
  - Owns the fixed evaluation metric and fixed runtime constants.
  - Explicitly says the evaluator should not change.
- `program.md`
  - Defines the autonomous experiment loop:
    - establish baseline
    - make a change
    - run evaluation
    - keep or discard
    - log results

The important pattern to copy is not “LLM training”. The important pattern is:

- narrow mutable surface
- immutable evaluator
- fixed corpus
- fixed metric
- append-only experiment log
- keep/discard based on a deterministic rule

## What The Current Repo Already Has

This repo already has a useful correctness harness in `evals/single_thread`.

Current properties:

- Single-thread deterministic harness only.
- Verifies:
  - `fastcore` deterministic regression report
  - normalized `initial-branch-analysis` CSV outputs for:
    - `baseline`
    - `blue2`
    - `orange2`
    - `white12`
- Supports seed/playout overrides via `SEED_COUNT`.
- Works for `500` playout validation today.

Important facts observed:

- `verify_against_golden.sh smoke` fails if no goldens exist yet.
- Goldens are not currently provisioned in the repo by default.
- `generate_golden.sh` currently benchmarks only one fixed board and one fixed state for `initial-branch-analysis`.
- CSV normalization intentionally strips timing columns like `WALL_TIME_SEC` and `CPU_TIME_SEC`.

Verified commands:

```bash
SEED_COUNT=500 evals/single_thread/generate_golden.sh smoke /tmp/catan-regression-500
SEED_COUNT=500 evals/single_thread/verify_against_golden.sh smoke /tmp/catan-regression-500 /tmp/catan-regression-500-verify
```

These completed successfully.

## Why The Current Harness Is Not Enough By Itself

It is a correctness gate, not a performance evaluator.

Problems if used alone:

- It produces pass/fail, not a scalar throughput objective.
- It explicitly throws away timing fields during normalization.
- Coverage is too narrow for “no result changes” across the real workload surface.
- It does not handle runtime noise.
- It does not define an experiment queue or acceptance threshold.

The harness is still valuable. It should remain the first gate every candidate must pass.

## Desired Optimization Contract

The swarm should optimize only under this contract:

- Single-thread behavior only.
- Exact results must match baseline on the declared correctness corpus.
- Performance score is measured only by the immutable benchmark harness.
- No candidate may change benchmark/evaluator scripts, corpus manifests, or result-logging schema.

## Immutable Vs Mutable Surface

This is the direct adaptation of the `autoresearch` pattern.

### Immutable

Agents must not modify:

- correctness harness scripts under `evals/single_thread/`
- benchmark scripts created by this plan
- corpus manifests
- baseline golden artifacts
- experiment log schema
- acceptance thresholds

### Mutable

Agents may modify only performance-relevant code, for example:

- `crates/fastcore/src/**`
- `crates/initial-branch-analysis/src/**`
- tightly related code paths required by those optimizations

Do not let agents casually edit docs, harness scripts, or evaluation manifests during optimization rounds.

## Proposed Evaluation Architecture

### 1. Candidate Generation Lanes

Run many agents in parallel.

Each agent gets:

- its own git worktree
- its own branch
- a narrow task scope:
  - “reduce allocation churn”
  - “improve action encoding hot path”
  - “remove redundant clones”
  - “optimize playout aggregation”
  - etc.

Each agent may:

- inspect profiles
- patch code
- build locally
- run lightweight correctness smoke checks
- commit one candidate

Each agent may not:

- rewrite the benchmark harness
- redefine success criteria
- overwrite another agent’s worktree

### 2. Correctness Lane

Candidates that compile must pass correctness before any performance evaluation.

Phase A:

- run the existing single-thread deterministic harness

Phase B:

- run expanded correctness cases on a broader corpus, not just the single example state

Correctness is binary:

- any diff means reject

### 3. Benchmark Lane

This must be serialized on one dedicated benchmark worker.

Do not run multiple final benchmarks at the same time on the same machine if the metric is single-CPU throughput.

The benchmark lane runs:

- one candidate at a time
- against the same baseline commit
- on the same fixed corpus
- with paired repeated measurements

## What “Paired” Means

Paired does not mean simultaneous.

It means:

1. run baseline on workload `W`
2. run candidate on workload `W`
3. compare those two numbers as one pair

Then repeat with order alternated:

1. run candidate on workload `W`
2. run baseline on workload `W`
3. compare again

Why:

- machine temperature drifts
- turbo behavior drifts
- background noise drifts
- caches warm differently over time

Pairing reduces drift because baseline and candidate are compared under near-identical conditions.

## Benchmark Metric

Primary metric:

- `cpu_ns_per_playout`

Equivalent reporting forms:

- `playouts_per_cpu_second`
- `cpu_seconds_per_1k_playouts`

Use CPU-time-derived metrics, not just wall time.

Still record wall time for diagnosis, but do not use wall time as the primary acceptance metric.

### Why CPU Time

CPU time is usually better than wall time for single-CPU benchmarking because it excludes time spent descheduled.

However, CPU time is still affected by shared-resource contention, so it is not enough to justify parallel final benchmarking.

## Hardware Counters

Optional, not primary.

Useful counters on Linux:

- retired instructions
- cycles
- task-clock
- cache-misses
- branch-misses

Interpretation:

- `instructions/playout` is a good low-noise proxy for machine work
- `cycles/playout` is closer to actual speed
- counters are diagnostic and secondary ranking signals

Do not accept candidates on retired instructions alone.

Why not:

- fewer instructions can still be slower
- memory behavior, cache locality, and SIMD efficiency can dominate

Important platform note:

- On Linux, `perf stat` makes this relatively straightforward.
- On macOS, hardware-counter automation is more cumbersome.
- If the benchmark host remains macOS, prefer isolated serialized timing first and treat counters as optional future work.

## Benchmark Protocol

### Fixed Conditions

Always benchmark with:

- release build
- `workers=1`
- fixed seeds
- fixed workload manifest
- fixed binary path or exact commit checkout
- no concurrent benchmark jobs on the same benchmark lane

Prefer:

- dedicated physical core
- avoid SMT sibling contention if possible
- stable power mode
- no unrelated heavy background jobs

### Warmup

Each case should include warmup before measurement.

Warmup purpose:

- page cache
- binary load
- allocator state
- branch predictor
- data structure hot paths

Do not count warmup in the score.

### Repeats

For each workload:

- run at least 5 paired measurements
- alternate order:
  - baseline -> candidate
  - candidate -> baseline
  - baseline -> candidate
  - candidate -> baseline
  - baseline -> candidate

### Aggregation

Per workload:

- compute paired improvement for each repeat
- aggregate using median paired improvement

Corpus-wide:

- aggregate per-workload medians into a global median or weighted median

### Acceptance Threshold

Do not accept tiny wins by default.

Recommended initial rule:

- correctness must pass exactly
- median corpus-wide improvement must be greater than `2%`
- no severe regression on any critical workload bucket

This threshold can be tuned later after noise is characterized.

## Correctness Corpus

The current harness is too narrow.

Expand correctness coverage to include:

- existing `fastcore` deterministic regression
- existing IBA scenarios:
  - baseline
  - blue2
  - orange2
  - white12
- multiple additional representative states/boards from:
  - `data/opening_states/states/`
  - `data/opening_states/boards/`

At minimum include cases that cover:

- default branch evaluation
- stackelberg modes
- white12 mode
- representative branch fanout sizes

If “no result changes” truly means “do not change user-visible outputs anywhere important”, the correctness corpus must be broader than one board and one state.

## Benchmark Corpus

The benchmark corpus should be representative of the hot paths you actually want to accelerate.

Recommended categories:

- `fastcore` pure playout-heavy cases
- `initial-branch-analysis` baseline cases
- `initial-branch-analysis` white12 cases
- low fanout cases
- medium fanout cases
- high fanout cases

Each workload entry should define:

- ID
- binary/command
- input files
- flags
- seeds
- expected playout count or comparable work unit count
- whether it is correctness-only, benchmark-only, or both

## Proposed Files To Add

These are suggested files for the implementation team.

```text
evals/single_thread/
  benchmark_manifest.json
  benchmark_candidate.sh
  benchmark_pair.sh
  benchmark_compare.py
  correctness_manifest.json
  run_correctness_suite.sh
  results/
    benchmark/
    correctness/
```

Suggested responsibilities:

- `benchmark_manifest.json`
  - fixed workload list
- `correctness_manifest.json`
  - exact correctness workload list
- `run_correctness_suite.sh`
  - immutable correctness entrypoint
- `benchmark_candidate.sh`
  - one candidate, one corpus pass, raw metrics only
- `benchmark_pair.sh`
  - execute paired baseline/candidate runs in alternating order
- `benchmark_compare.py`
  - compute medians, deltas, acceptance decision, and leaderboard row

## Experiment Log

Copy the `autoresearch` idea of an append-only experiment log.

Do not make the agents invent their own logging format.

Recommended TSV columns:

```text
timestamp	candidate_branch	candidate_commit	base_commit	correctness_status	median_speedup_pct	min_speedup_pct	max_slowdown_pct	cpu_ns_per_playout	status	description
```

Status values:

- `keep`
- `discard`
- `correctness_fail`
- `build_fail`
- `benchmark_fail`

Every candidate should also archive:

- raw correctness artifacts
- raw benchmark measurements
- computed summary

## Worktree Conventions

Each swarm agent should work in its own git worktree.

Recommended naming:

- branch: `perf/<date>/<agent-name>/<short-topic>`
- worktree dir: `/tmp/catan-perf-<agent-name>`

Example:

- branch: `perf/2026-03-14/agent07/remove-clones`
- worktree: `/tmp/catan-perf-agent07`

Each candidate should end with:

- one commit
- one short description
- one enqueue event into the benchmark lane

## Swarm Roles

### Controller

- creates worktrees
- assigns tasks
- tracks candidate queue
- records keep/discard decisions

### Candidate Agents

- make code changes only in mutable surface
- run local build
- run lightweight prechecks
- commit candidate
- enqueue for evaluation

### Benchmark Worker

- only worker allowed to run final benchmark measurements
- checks out baseline and candidate commits
- runs correctness suite
- runs paired benchmark protocol
- writes decision row

## Recommended Agent Prompt Rules

Every swarm agent should be told:

- optimize single-CPU throughput only
- do not change observable results
- do not modify evaluator scripts
- do not modify corpus manifests
- do not change acceptance thresholds
- avoid broad refactors unless justified by hotspot evidence
- prefer simple wins over clever complexity

## Suggested Development Workflow

1. Seed baseline goldens for the declared correctness corpus.
2. Freeze evaluator files.
3. Build the benchmark lane.
4. Characterize baseline noise on the benchmark machine.
5. Set acceptance threshold.
6. Launch swarm worktrees.
7. Let candidates run correctness in parallel.
8. Feed correctness-pass commits into the serialized benchmark lane.
9. Keep only candidates that pass exact correctness and exceed the improvement threshold.
10. Periodically rebase or roll baseline forward only after a deliberate decision.

## Roll-Forward Policy

Do not automatically benchmark candidate changes against the immediately previous candidate unless that is an explicit controller decision.

Preferred policy:

- benchmark against a stable baseline commit
- accept one winner
- advance baseline deliberately
- restart the next wave from that new baseline

This keeps comparisons cleaner and avoids a noisy moving target.

## Risk Controls

- Do not trust single-run timings.
- Do not benchmark candidates concurrently on the same benchmark lane.
- Do not let agents change evaluator code.
- Do not let “small likely wins” bypass correctness.
- Do not use instruction count alone as the success criterion.
- Do not assume the current single-board harness proves whole-repo equivalence.

## Minimal First Milestone

The smallest useful version of this plan is:

1. Keep the existing single-thread correctness harness.
2. Add 3 to 5 more representative workload cases.
3. Add one immutable benchmark script that reports `cpu_ns_per_playout`.
4. Run 5 paired repeats per candidate.
5. Accept only candidates with exact correctness and `>2%` median improvement.
6. Benchmark only one candidate at a time.

This is enough to start safe optimization work without waiting for a perfect system.

## Final Recommendation

Model the system after `autoresearch` structurally, not cosmetically.

What to copy:

- immutable evaluator
- append-only experiment log
- keep/discard loop
- narrow mutable surface
- fixed metric

What not to copy literally:

- GPU training setup
- 5-minute wall-clock budget
- single editable file

For this repo, the right equivalent is:

- exact correctness gate
- dedicated serialized benchmark lane
- paired repeated CPU-time measurements
- swarm-generated candidates in isolated worktrees
- strict keep/discard policy based on immutable scripts
# Superseded

This document reflects an older evaluator design that included `initial-branch-analysis`.
The active harness under `evals/single_thread/` is now playout-only (`fastcore` correctness plus `bench_value_state` benchmark).
