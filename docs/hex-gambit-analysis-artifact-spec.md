# White12 Holdout CSV Spec (Hex Gambit)

Status: Active  
Owner: Engineering  
Last Updated: 2026-03-23

## 1. Goal

Define the current analysis artifact contract consumed by Hex Gambit runtime and adjacent tooling.

Primary runtime artifact:

- `initial_branch_analysis_all_sims_holdout.csv`

Preferred transport form:

- `initial_branch_analysis_all_sims_holdout.csv.gz`

## 2. Scope

In scope:

- WHITE12 holdout all-sims CSV rows.
- `.csv.gz` primary and `.csv` fallback load behavior.
- Runtime ranking fields and follower-placement fields used by Hex Gambit.

Out of scope:

- `initial_branch_analysis.csv` as Hex Gambit runtime input.
- Custom binary conversion format.
- Non-holdout analysis ingestion for Hex Gambit.

## 3. Canonical Paths and Coverage

1. Runtime path pattern:
- `runtime-data/opening_states/<id>/initial_branch_analysis_all_sims_holdout.csv(.gz)`

2. Current repository state:
- Opening-state sample index (`data/opening_states/index.json`) contains `0001..0012`.
- Tracked runtime analysis directories currently contain `0001..0009`.
- Current Hex Gambit board payload consumes `0001..0008`.

3. Source-generation roots:
- Analysis outputs are generated under `data/analysis/opening_states/<id>/`.
- Runtime-consumed assets are synchronized under `runtime-data/opening_states/<id>/`.

## 4. Required and Optional Columns

Required sequence-identifying columns:

- `LEADER_SETTLEMENT`
- `LEADER_ROAD`
- `LEADER_SETTLEMENT2`
- `LEADER_ROAD2`
- `WIN_WHITE`

Required for weighted aggregation when available:

- `SIMS_RUN` (runtime falls back to unweighted averaging if missing/invalid)

Optional but consumed when present:

- `WIN_RED`, `WIN_BLUE`, `WIN_ORANGE`, `WIN_WHITE` (per-color win bars)
- follower-placement columns (`FOLLOWER*`) used in board-result reveal

Optional leader-conditioned win-stat columns:

- `WIN_<LEADER>_PCT_HAS_SETTLEMENT`
- `WIN_<LEADER>_AVG_SETTLEMENTS`
- `WIN_<LEADER>_AVG_CITIES`
- `WIN_<LEADER>_PCT_HAS_CITY`
- `WIN_<LEADER>_PCT_HAS_VP`
- `WIN_<LEADER>_AVG_VP_GIVEN_HAS`
- `WIN_<LEADER>_PCT_LA`
- `WIN_<LEADER>_PCT_LR`
- `WIN_<LEADER>_PCT_BOTH`
- `WIN_<LEADER>_PCT_PLAYED_MONOPOLY`
- `WIN_<LEADER>_PCT_PLAYED_YOP`
- `WIN_<LEADER>_PCT_PLAYED_ROAD_BUILDER`
- `WIN_<LEADER>_PCT_PLAYED_KNIGHTS`
- `WIN_<LEADER>_AVG_KNIGHTS_GIVEN_PLAYED`
- `WIN_<LEADER>_AVG_TURN_FIRST_CITY`
- `WIN_<LEADER>_AVG_TURN_FIRST_SETTLEMENT`

## 5. Runtime Load Contract

For each board:

1. Try `initial_branch_analysis_all_sims_holdout.csv.gz`.
2. Fall back to raw `.csv` when `.gz` is unavailable or unsupported.
3. Parse rows into deterministic sequence aggregates.
4. Rank sequences by WHITE win% descending with canonical tuple tie-break.
5. Build rank map and lookup map used by board result and signal inference.

No binary post-processing step is required by Hex Gambit runtime.

## 6. Acceptance Criteria

1. Hex Gambit completes sessions using holdout CSV artifacts only.
2. `.csv.gz` and `.csv` yield equivalent ranked sequence results for identical content.
3. Identical input rows produce identical rank ordering.
4. Missing sequence rows for a chosen completed path fail explicitly (no silent default result).

## 7. Mode Safety: Replay vs Rerun

Canonical intent model is documented in:

- `docs/holdout-modes-runbook.md`

Current CLI compatibility behavior remains:

- `--holdout-rerun`: recompute holdout summaries.
- `--holdout-replay <path>`: replay explicit rows from a replay CSV.

Safety rule:

- Replay intent requires `--holdout-replay <path>`.
- Rerun intent must not substitute replay input.
