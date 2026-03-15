# Single-Thread Deterministic Harness

This harness validates deterministic behavior for `workers=1` only.

## What it checks

- `fastcore` aggregate stats plus per-seed trajectory fingerprints
- `initial-branch-analysis` CSV outputs for:
  - baseline
  - `--blue2`
  - `--orange2`
  - `--white12`
- normalization strips timing/host/worker columns before CSV diffing

## Commands

Generate goldens:

```bash
evals/single_thread/generate_golden.sh smoke
```

Verify current code against goldens:

```bash
evals/single_thread/verify_against_golden.sh smoke
```

Suites:

- `smoke`: 64 seeds
- `gate`: 2048 seeds
- `deep`: 20000 seeds

Optional env overrides:

- `SEED_COUNT`
- `START_SEED`
- `MAX_TURNS`
- `BRANCH_LIMIT` (passed to `initial-branch-analysis --limit`)

## Branch Fanout (Important)

`initial-branch-analysis` runtime is dominated by leader branch fanout, not only by `--num-sims`.

- Each CSV row is one evaluated leader branch.
- Branch count can be read directly as `line_count - 1` from each scenario CSV.
- The harness defaults `BRANCH_LIMIT=32` for `smoke` so runs finish quickly.
- `gate` and `deep` default to full fanout unless `BRANCH_LIMIT` is set.

Get branch counts from generated artifacts:

```bash
evals/single_thread/branch_counts.sh /path/to/artifacts
```
