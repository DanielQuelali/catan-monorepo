# Documentation Drift Register

Status: Active  
Owner: Engineering  
Last Updated: 2026-03-23

## Purpose

Track doc/code mismatches, evidence, and remediation status.

## Drift log

| ID | Previous documentation state | As-built evidence | Resolution status |
|---|---|---|---|
| D-001 | `crates/fastcore/README.md` referenced a missing overhaul document and described scaffold-only status. | Missing-file reference plus current exported modules/binaries in `crates/fastcore/src/lib.rs`. | Resolved by rewriting `crates/fastcore/README.md` to current capabilities and commands. |
| D-002 | Root `README.md` listed only a subset of active surfaces and omitted viewer/eval/automation entrypoints. | Active directories and commands include `apps/opening-board-viewer`, `evals/single_thread`, and multiple `scripts/` operators. | Resolved by expanding root `README.md` surface map + quick-start commands. |
| D-003 | Runtime app docs were unevenly detailed and missing a diagnostics-app README. | `apps/opening-board-viewer` had no local README while other surfaces had explicit run guidance. | Resolved by adding `apps/opening-board-viewer/README.md` and normalizing root entrypoints. |
| D-004 | `apps/opening-board-viewer` had no local README. | App is runnable via `serve.mjs`; query params (`data`, `analysis`) are implemented in `app.js`. | Resolved by adding `apps/opening-board-viewer/README.md`. |
| D-005 | Product + technical Hex Gambit docs specified fixed 2-board session behavior. | `apps/hex-gambit/app.js` uses `MODEL.boards.length`; `boards.json` currently contains 8 boards (`0001..0008`). | Resolved by aligning PRD + technical spec to current board-set behavior. |
| D-006 | Technical spec described binary board payload contract and manifest shape not used by current app. | Hex Gambit loads `apps/hex-gambit/data/boards.json` + holdout CSV artifacts (`initial_branch_analysis_all_sims_holdout.csv(.gz)`). | Resolved by replacing stale contract sections with current payload/CSV contracts. |
| D-007 | Artifact spec implied full `0001..0012` runtime analysis availability without clarifying tracked subset. | `data/opening_states/index.json` has 12 samples; `runtime-data/opening_states/` currently has `0001..0009`. | Resolved by updating artifact spec to separate sample universe vs tracked runtime assets. |
| D-008 | No docs-level active/superseded navigation index. | Multiple docs are marked superseded or planned; canonical entrypoint was unclear. | Resolved by adding `docs/README.md` index with active/planned/superseded map. |
