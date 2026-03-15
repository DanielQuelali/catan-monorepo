# Single-Thread Playout Harness

This harness validates and benchmarks the single playout unit only.

## What it checks

- correctness is `fastcore` deterministic replay only
- benchmark is `fastcore` `bench_value_state` only
- all runs are `workers=1`
- the primary performance metric is `cpu_ns_per_playout`
- summaries also report total playouts and `playouts_per_cpu_second`

## Commands

Generate goldens:

```bash
evals/single_thread/generate_golden.sh smoke
```

Verify current code against goldens:

```bash
evals/single_thread/verify_against_golden.sh smoke
```

Manifest-driven correctness:

```bash
evals/single_thread/run_correctness_suite.sh generate --suite smoke --out-dir /tmp/catan-correctness-smoke
evals/single_thread/run_correctness_suite.sh verify --suite smoke --golden-dir /tmp/catan-correctness-smoke --work-dir /tmp/catan-correctness-smoke-verify
```

Paired benchmark lane:

```bash
evals/single_thread/benchmark_pair.sh \
  --baseline-root /tmp/catan-base \
  --candidate-root /tmp/catan-candidate \
  --suite smoke \
  --golden-dir /tmp/catan-correctness-smoke \
  --out-dir /tmp/catan-benchmark-pair
```

Suites:

- `smoke`: correctness `64` seeds, benchmark `4 x 512` playouts
- `gate`: correctness `2048` seeds, benchmark `4 x 2048` playouts
- `deep`: correctness `20000` seeds, benchmark `4 x 20000` playouts

Optional correctness env overrides:

- `SEED_COUNT`
- `START_SEED`
- `MAX_TURNS`

## Immutable Evaluator Files

- `generate_golden.sh`
- `verify_against_golden.sh`
- `correctness_manifest.json`
- `run_correctness_suite.sh`
- `benchmark_manifest.json`
- `benchmark_candidate.sh`
- `benchmark_pair.sh`
- `benchmark_compare.py`

Rules for optimization worktrees:

- Treat those evaluator files and manifests as immutable.
- Run candidate generation and correctness in parallel if you want.
- Keep final benchmark measurement serialized on one benchmark lane.
- The benchmark lane uses paired repeated runs and accepts only candidates that pass correctness and beat the fixed 5% threshold.

## Campaign Workflow

The `autoresearch`-style controller is `evals/single_thread/perf_campaign.sh`.

Initialize one campaign:

```bash
evals/single_thread/perf_campaign.sh init \
  --campaign-id gate-20260314 \
  --baseline-ref HEAD \
  --benchmark-suite gate \
  --correctness-suite gate \
  --generate-goldens
```

Create one candidate branch/worktree:

```bash
evals/single_thread/perf_campaign.sh create-candidate \
  --campaign-id gate-20260314 \
  --agent-id worker-01 \
  --topic fastcore-rollout-loop
```

Submit candidate after committing in its worktree:

```bash
evals/single_thread/perf_campaign.sh submit-candidate \
  --campaign-id gate-20260314 \
  --candidate-id <candidate-id>
```

Run the serialized benchmark worker:

```bash
evals/single_thread/perf_campaign.sh worker-run \
  --campaign-id gate-20260314 \
  --drain
```

Run as a long-lived unattended worker:

```bash
evals/single_thread/perf_campaign.sh worker-run \
  --campaign-id gate-20260314 \
  --drain \
  --continuous \
  --poll-seconds 15
```

Show autoresearch-style results summary:

```bash
evals/single_thread/perf_campaign.sh results --campaign-id gate-20260314
```

Advance baseline explicitly after a `keep` decision:

```bash
evals/single_thread/perf_campaign.sh advance-baseline \
  --campaign-id gate-20260314 \
  --candidate-id <candidate-id>
```

Inspect status and queue:

```bash
evals/single_thread/perf_campaign.sh status --campaign-id gate-20260314 --show-items
```

Campaign presentation files:

- `campaign/results.tsv`
- `campaign/analysis.ipynb`
- `ledger/experiments.jsonl`

Default worker hard timeouts:

- correctness: `1800` seconds
- benchmark: `3600` seconds
