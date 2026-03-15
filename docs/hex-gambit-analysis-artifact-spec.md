# White12 Holdout CSV Spec (Hex Gambit)

Status: Draft  
Owner: Engineering  
Last Updated: 2026-03-03

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
- `runtime-data/opening_states/<id>/initial_branch_analysis_all_sims_holdout.csv`

Hex Gambit should read this schema (direct CSV or `.csv.gz` equivalent).

## 4. Required Columns

- `LEADER_SETTLEMENT`, `LEADER_ROAD`
- `FOLLOWER1_SETTLEMENT`, `FOLLOWER1_ROAD` (WHITE second)
- `FOLLOWER2_SETTLEMENT`, `FOLLOWER2_ROAD` (ORANGE)
- `FOLLOWER3_SETTLEMENT`, `FOLLOWER3_ROAD` (BLUE)
- `FOLLOWER4_SETTLEMENT`, `FOLLOWER4_ROAD` (RED)
- `WIN_WHITE`
- `SIMS_RUN`

Optional leader-conditioned win-condition columns (holdout rows only):
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

All optional `WIN_<LEADER>_*` columns are conditioned on the leader winning that playout.

## 5. Runtime Contract

1. For selected board `<id>`, load:
- `runtime-data/opening_states/<id>/initial_branch_analysis_all_sims_holdout.csv.gz`
2. If `.gz` is unavailable, load raw `.csv` fallback.
3. Parse the CSV rows and build ranking/index structures in memory.

No conversion step to separate binary artifacts is required.

## 6. Acceptance Criteria

1. Hex Gambit works using only holdout CSV data.
2. No dependency on non-holdout analysis files.
3. `.csv.gz` and `.csv` produce equivalent parsed rows.
4. Ranking is deterministic for the same holdout input.

## 7. Resume Note (Paused Run)

Paused run (stopped on 2026-02-22) used:
- `--holdout-only --holdout-rerun --budget 3000 --num-sims 200 --exclude-sample-ids 0001`
- Log: `data/analysis/opening_states/holdout_except_0001_b3000_n200_20260222_143152.log`
- Last started sample in log: `0010`

Interpretation of that paused command:
- Mode intent: `rerun` (via legacy `--holdout-rerun`)
- Output scope: `holdout` (via legacy `--holdout-only`)

After Mac reboot, resume in tmux by rerunning remaining boards (safe to rerun `0010`):

```bash
tmux new -d -s holdout_resume_$(date +%Y%m%d_%H%M%S) \
'cd /Users/daniel/Projects/catan-monorepo && \
python3 scripts/run_opening_white12_analysis.py \
  --holdout-only --holdout-rerun --budget 3000 --num-sims 200 \
  --exclude-sample-ids 0001,0002,0003,0004,0005,0006,0007,0008,0009 \
  > data/analysis/opening_states/holdout_resume.log 2>&1'
```

## 8. Mode Safety: Replay vs Rerun (Critical)

Canonical model (runbook):
- `--holdout-mode replay|rerun|reuse`
- `--all-sims-scope all|holdout`

Current CLI still commonly uses legacy flags, but semantics remain strict:

- `--holdout-rerun`: recomputes holdout summaries (not replay-file mode)
- `--holdout-replay <path>`: replays explicit rows from a replay CSV

If a request says "replay", command must include `--holdout-replay <path>`.
Do not substitute `--holdout-rerun`.

Authoritative references:
- `docs/holdout-modes-runbook.md`
- `docs/holdout-parameter-overhaul-plan-2026-03-03.md`
- `docs/holdout-replay-incident-postmortem-2026-03-03.md`
