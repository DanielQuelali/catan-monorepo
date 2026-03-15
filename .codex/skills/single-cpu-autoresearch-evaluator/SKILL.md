---
name: single-cpu-autoresearch-evaluator
description: Run the repo's immutable single-CPU playout evaluator for swarm/autoresearch work. Use when seeding correctness goldens, validating a candidate worktree, benchmarking baseline vs candidate with paired repeated runs, or enforcing the rule that evaluator scripts and manifests stay fixed while agents optimize playout throughput without changing results.
---

# Single Cpu Autoresearch Evaluator

Use this skill for the `autoresearch`-style loop in this repo: fixed evaluator, narrow mutable surface, keep/discard decisions, append-only results.

The reason for the evaluator is the same as in `/tmp/autoresearch`:
- the optimizer should change the implementation, not the judge
- the metric must stay fixed across experiments
- candidates should be accepted or rejected by a simple deterministic rule

In this repo, the immutable evaluator lives under `evals/single_thread/`.

## Immutable Surface

Treat these as fixed during optimization rounds:
- `evals/single_thread/correctness_manifest.json`
- `evals/single_thread/benchmark_manifest.json`
- `evals/single_thread/run_correctness_suite.sh`
- `evals/single_thread/benchmark_candidate.sh`
- `evals/single_thread/benchmark_pair.sh`
- `evals/single_thread/benchmark_compare.py`
- existing golden artifacts for the chosen correctness suite
- acceptance thresholds and logging schema

If the user wants to change manifests, thresholds, or workload selection, treat that as evaluator-maintenance work, not as part of a candidate optimization round.

Mutable surface is usually limited to:
- `crates/fastcore/src/**`

## Core Files

Read these first when you need context:
- `evals/single_thread/README.md`
- `docs/single-cpu-autoresearch-plan-2026-03-14.md`

Use these as the operational entrypoints:
- `evals/single_thread/run_correctness_suite.sh`
- `evals/single_thread/benchmark_pair.sh`

The manifests define the fixed corpus:
- `evals/single_thread/correctness_manifest.json`
- `evals/single_thread/benchmark_manifest.json`

## Workflow

### 1. Seed correctness goldens

Generate goldens before any candidate verification if they do not already exist:

```bash
evals/single_thread/run_correctness_suite.sh generate \
  --suite smoke \
  --out-dir /tmp/catan-correctness-smoke
```

Use `smoke` for harness debugging and quick iteration. Use `gate` for real candidate acceptance unless the user explicitly asks for a different suite.

### 2. Verify a candidate worktree

Run correctness against the fixed golden directory:

```bash
evals/single_thread/run_correctness_suite.sh verify \
  --root /tmp/catan-perf-agent07 \
  --suite gate \
  --golden-dir /tmp/catan-correctness-gate \
  --work-dir /tmp/catan-correctness-agent07
```

Any diff is a rejection. Do not benchmark a candidate that fails correctness.

On correctness failure, inspect the candidate work dir for `failure_dump/`. The verifier writes:
- `engine_report.diff`
- `mismatch_summary.json`
- `candidate_logs/seed_<N>.log` for the first mismatching seed

### 3. Run the serialized benchmark lane

Benchmark one candidate worktree against one baseline worktree:

```bash
evals/single_thread/benchmark_pair.sh \
  --baseline-root /tmp/catan-base \
  --candidate-root /tmp/catan-perf-agent07 \
  --suite gate \
  --golden-dir /tmp/catan-correctness-gate \
  --out-dir /tmp/catan-bench-agent07 \
  --description "remove redundant clones in playout hot path"
```

Default behavior:
- verify correctness first
- run 5 paired repeats
- alternate baseline/candidate order
- write `summary.json` under the output directory
- keep threshold defaults to 5% unless overridden

Use `--skip-correctness` only when debugging the harness itself or doing a controlled self-compare.

## Interpretation

Read `summary.json` after the pair run:
- `status=keep` means correctness passed and median corpus-wide speedup beat the threshold
- `status=discard` means the candidate was slower, too noisy, or regressed a critical workload
- `status=correctness_fail` means the candidate should be rejected immediately

Primary metrics:
- `cpu_ns_per_playout`
- `playouts_per_cpu_second`
- total playout counts per repeat and across the paired run

Lower `cpu_ns_per_playout` is better. Higher `playouts_per_cpu_second` is better.

Paired improvement is computed against the baseline per workload. Positive `median_speedup_pct` means the candidate is faster.

## Benchmark Discipline

Do not run multiple final benchmark jobs at the same time on the same benchmark machine.

Parallelize:
- candidate generation
- local builds
- lightweight smoke checks
- correctness verification on separate workers if needed

Serialize:
- final `benchmark_pair.sh` runs on the benchmark lane

Reason:
- shared cache, memory bandwidth, scheduler drift, and thermal drift will corrupt single-CPU timing

## Worktree Rules

Use separate worktrees per agent, for example:
- baseline: `/tmp/catan-base`
- candidate: `/tmp/catan-perf-agent07`

Keep the decision loop simple, following the `autoresearch` pattern:
1. make one performance change
2. verify correctness
3. benchmark against baseline
4. keep or discard
5. log the result

Do not let candidate agents edit the evaluator while they are being judged by it.
