# Catan Monorepo Current-State Baseline

Status: Active  
Owner: Engineering  
Last Updated: 2026-03-23

## 1. Snapshot scope

This document records the as-built state of repository surfaces, runtime data contracts, evaluator tooling, and operator automation as of the update date above.

## 2. Repository surfaces

1. Simulation core
- `crates/fastcore`
- Deterministic engine + value player + regression/benchmark binaries.

2. Analysis CLI
- `crates/initial-branch-analysis`
- Opening-branch CSV generation, holdout modes, stackelberg controls.

3. Runtime apps
- `apps/hex-gambit` (static app; launched by `./run-hex-gambit.sh`)
- `apps/opening-board-viewer` (static diagnostics app; served by `node apps/opening-board-viewer/serve.mjs`)

4. Evaluation harness
- `evals/single_thread`
- Deterministic correctness + serialized benchmark campaign controller.

5. Operator automation
- `scripts/run_opening_white12_analysis.py`
- `scripts/generate_opening_states.py`
- `scripts/ceodex_*` supervisor/producer/worker/watchdog scripts.

## 3. Hex Gambit runtime behavior

1. Session and board flow
- Board payload source: `apps/hex-gambit/data/boards.json`
- Current board set in payload: `0001..0008` (8 boards).
- Session iterates through all payload boards; progression is not hardcoded to 2 boards.
- Per-board step flow is 4 steps (`s1`, `e1`, `s2`, `e2`), derived from `meta.sequenceKeys` when present.

2. Analysis ingestion
- Default analysis root: `./runtime-data/opening_states`
- Per-board analysis candidates:
  - `initial_branch_analysis_all_sims_holdout.csv.gz`
  - fallback `initial_branch_analysis_all_sims_holdout.csv`
- Result ranking is deterministic by:
  - WHITE win percent descending
  - sequence tuple tie-break ascending.

3. UX and state
- Stages: `loading`, `intro`, `placement`, `board_result`, `summary`, `error`.
- In-memory session state only; no cross-session persistence.
- Completed board replay is not exposed in-session; flow is continue-forward or full restart.

## 4. Opening-state and analysis data flow

1. Fixture generation
- `scripts/generate_opening_states.py` writes:
  - `data/opening_states/boards/board_*.json`
  - `data/opening_states/states/state_*.json`
  - `data/opening_states/index.json`

2. Analysis generation
- `crates/initial-branch-analysis` produces per-state CSV artifacts.
- Batch wrapper `scripts/run_opening_white12_analysis.py` orchestrates per-sample runs.

3. Current artifact counts in repo
- `data/opening_states/index.json` currently lists 12 samples (`0001..0012`).
- `runtime-data/opening_states/` currently contains tracked analysis directories `0001..0009`.
- Hex Gambit currently consumes `0001..0008`, which are present in runtime assets.

## 5. Evaluator and campaign operations

1. Canonical harness
- `evals/single_thread/README.md` is the canonical operating guide.
- Evaluator is intentionally single-thread (`workers=1`) for correctness and benchmark comparability.

2. Campaign controller
- Entry command: `evals/single_thread/perf_campaign.sh <subcommand>`
- Controller implementation: `evals/single_thread/perf_campaign.py`
- Core subcommands: `init`, `status`, `results`, `create-candidate`, `submit-candidate`, `worker-run`, `advance-baseline`, `cleanup-candidate`, `cancel-candidate`.

3. CEOdex automation surface
- Supervisor: `scripts/ceodex_campaign.py`
- Launch helpers: `scripts/start_ceodex_campaign.sh`, `scripts/ceodex_launchd_supervisor.sh`
- Loop scripts: `scripts/ceodex_producer_loop.sh`, `scripts/ceodex_worker_loop.sh`, `scripts/ceodex_cleanup_loop.sh`
- Restart automation: `scripts/ceodex_cron_watchdog.sh`, installer scripts for cron/launchd.

## 6. Deployment shape

1. GitHub Pages workflow
- `.github/workflows/deploy-pages.yml` packages and deploys `apps/hex-gambit` plus `runtime-data/opening_states`.
- Runtime-data is included in deployment artifact under `_site/runtime-data/opening_states`.

2. Local static serving
- `./run-hex-gambit.sh` serves `apps/hex-gambit` via `python3 -m http.server` (default port `8080`).
- `apps/opening-board-viewer/serve.mjs` serves repo-root static paths (default `127.0.0.1:8091`).

## 7. Known constraints and gaps

1. Test coverage shape
- Strong Rust core coverage in `crates/fastcore/tests`.
- No committed automated test suite for `apps/hex-gambit` or `apps/opening-board-viewer`.

2. Holdout-mode interface transition
- Runbook defines canonical `--holdout-mode replay|rerun|reuse`, but active CLIs still expose legacy flags (`--holdout-rerun`, `--holdout-only`, etc.).

3. Data inventory mismatch
- Opening-state sample index includes 12 samples while tracked runtime analysis directories are currently 9 (`0001..0009`).
