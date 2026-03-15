---
name: single-thread-regression-harness
description: Run the repo's deterministic single-thread playout harness for `fastcore` only. Use when verifying a checkout against known-good artifacts, regenerating goldens, checking whether a custom playout count is supported, or running paired playout benchmarks with totals and throughput in the summary.
---

# Single Thread Regression Harness

Use `evals/single_thread` as the deterministic regression and benchmark path for the single playout unit.

This harness validates `workers=1` only. It does not cover multithreaded behavior, UI regressions, or evaluator-specific overhead.

## Quick Run

Verify against existing goldens:

```bash
evals/single_thread/verify_against_golden.sh smoke
```

If the repo does not already have goldens for that suite, generate them first:

```bash
evals/single_thread/generate_golden.sh smoke
```

Artifacts include:
- `engine_report.json` from `fastcore --bin deterministic_regression`
- `run_config.txt`

On correctness failure, the verifier now also writes a failure dump under the candidate work dir:
- `failure_dump/engine_report.diff`
- `failure_dump/mismatch_summary.json`
- `failure_dump/candidate_logs/seed_<N>.log` for the first mismatching seed

## Workflow

1. Check whether `evals/artifacts/golden/single_thread/<suite>` exists.
2. If it does not exist, generate a baseline first. Use a temporary output directory when you only want to test support or runtime:

```bash
evals/single_thread/generate_golden.sh smoke /tmp/catan-regression-smoke
```

3. For an actual regression check, run `verify_against_golden.sh`. It regenerates candidate artifacts under `evals/artifacts/verify/single_thread/<suite>` and diffs them against the golden directory.
4. If verification fails, inspect `engine_report.json` for aggregate drift, winner drift, turn-count drift, illegal-action drift, or per-seed trace hash drift.
5. If the verifier produced `failure_dump/`, start with `mismatch_summary.json` and the first mismatching seed log in `candidate_logs/`.

## Suites

- `smoke`: 64 correctness seeds, 4 x 512 benchmark playouts
- `gate`: 2048 correctness seeds, 4 x 2048 benchmark playouts
- `deep`: 20000 correctness seeds, 4 x 20000 benchmark playouts

## Custom Playout Counts

There is no named `500` suite. Use environment overrides for correctness goldens, or edit the benchmark manifest as evaluator maintenance.

Generate 500-playout correctness artifacts:

```bash
SEED_COUNT=500 evals/single_thread/generate_golden.sh smoke /tmp/catan-regression-500
```

Verify 500-playout artifacts against an existing golden directory:

```bash
SEED_COUNT=500 evals/single_thread/verify_against_golden.sh smoke /path/to/goldens /tmp/catan-regression-500-verify
```

The correctness harness passes `SEED_COUNT` into:
- `fastcore --bin deterministic_regression --seed-count <N>`

## Benchmark Output

Paired benchmark summaries now include:
- `baseline_total_playouts_per_repeat`
- `candidate_total_playouts_per_repeat`
- `total_paired_playouts_all_repeats`
- `baseline_playouts_per_cpu_second`
- `candidate_playouts_per_cpu_second`

## Limits

- `verify_against_golden.sh` exits immediately if the golden directory is missing.
- The harness is deterministic only for single-threaded runs; do not treat it as coverage for `workers>1`.
- Final single-CPU benchmark runs should stay serialized on one benchmark lane.
- The failure dump contains candidate raw logs for the first mismatching seed, not full golden-side raw logs.
