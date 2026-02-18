# White12 Holdout CSV Spec (Hex Gambit)

Status: Draft  
Owner: Engineering  
Last Updated: 2026-02-18

## 1. Goal

Hex Gambit should use only the WHITE12 holdout dataset.

Required source format:
- `initial_branch_analysis_all_sims_holdout.csv`

Transport/storage format:
- `initial_branch_analysis_all_sims_holdout.csv.gz` (preferred)

No additional analysis file types are required.

## 2. Scope (Strict)

In scope:
- WHITE12 only
- Holdout CSV only
- Optional gzip compression of that same CSV

Out of scope:
- `initial_branch_analysis.csv`
- TS/all-sims CSV ingestion
- Custom binary format
- Manifest/course layout

## 3. Canonical Input Path

For board `<id>` (`0001`..`0012`):
- `data/analysis/opening_states/<id>/initial_branch_analysis_all_sims_holdout.csv`

Hex Gambit should read this schema (direct CSV or `.csv.gz` equivalent).

## 4. Required Columns

- `LEADER_SETTLEMENT`, `LEADER_ROAD`
- `FOLLOWER1_SETTLEMENT`, `FOLLOWER1_ROAD` (WHITE second)
- `FOLLOWER2_SETTLEMENT`, `FOLLOWER2_ROAD` (ORANGE)
- `FOLLOWER3_SETTLEMENT`, `FOLLOWER3_ROAD` (BLUE)
- `FOLLOWER4_SETTLEMENT`, `FOLLOWER4_ROAD` (RED)
- `WIN_WHITE`
- `SIMS_RUN`

## 5. Runtime Contract

1. For selected board `<id>`, load:
- `data/analysis/opening_states/<id>/initial_branch_analysis_all_sims_holdout.csv.gz`
2. If `.gz` is unavailable, load raw `.csv` fallback.
3. Parse the CSV rows and build ranking/index structures in memory.

No conversion step to separate binary artifacts is required.

## 6. Acceptance Criteria

1. Hex Gambit works using only holdout CSV data.
2. No dependency on non-holdout analysis files.
3. `.csv.gz` and `.csv` produce equivalent parsed rows.
4. Ranking is deterministic for the same holdout input.
