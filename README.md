# catan-monorepo

Monorepo for Catan simulation, analysis, UI surfaces, and evaluation automation.

## Repository surfaces

- `crates/fastcore`: deterministic simulation core and benchmarking/regression binaries.
- `crates/initial-branch-analysis`: analysis CLI for opening-placement branches.
- `apps/hex-gambit`: standalone opening-placement evaluator web app.
- `apps/opening-board-viewer`: diagnostics viewer for opening-state boards + holdout analysis.
- `evals/single_thread`: deterministic correctness + serialized benchmark harness.
- `scripts/`: analysis wrappers, campaign automation, and operator tooling.

## Quick start

Hex Gambit from repo root:

```bash
./run-hex-gambit.sh
```

Opening Board Viewer from repo root:

```bash
node apps/opening-board-viewer/serve.mjs --host 127.0.0.1 --port 8091
```

Initial branch analysis from repo root:

```bash
cargo run -p initial-branch-analysis -- --white12 --budget 10 --holdout-sims 0
```

Batch opening-state analysis wrapper:

```bash
python3 scripts/run_opening_white12_analysis.py --help
```

Single-CPU campaign harness:

```bash
evals/single_thread/perf_campaign.sh --help
```

## Data roots

- Input fixtures: `data/`
- Analysis outputs: `data/analysis/`
- Runtime analysis assets served to static apps: `runtime-data/`

## Documentation index

See `docs/README.md` for active vs superseded docs and current-state references.
