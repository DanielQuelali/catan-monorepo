# Holdout Modes and Commands Runbook

Status: Active (intent model). CLI rollout in progress.  
Owner: Engineering  
Last Updated: 2026-03-03

## 1. Purpose

Prevent replay/rerun confusion in holdout artifact generation by forcing explicit mode selection and preflight checks.

Incident reference:
- `docs/holdout-replay-incident-postmortem-2026-03-03.md`

## 2. Canonical Parameter Model (Target Interface)

- `--holdout-mode replay|rerun|reuse`
- `--holdout-replay <path>` (required for `replay`)
- `--num-sims <N>` (required for `rerun`, `N > 0`)
- `--all-sims-scope all|holdout`

Compatibility aliases to deprecate:
- `--holdout-rerun` -> `--holdout-mode rerun`
- `--holdout-only` -> `--all-sims-scope holdout`
- `--holdout-sims` -> `--num-sims`

## 3. Truth Table (Authoritative)

| User intent | Required mode | Required flags | Forbidden flags |
|---|---|---|---|
| Replay rows from CSV | `replay` | `--holdout-replay <path>` | `--holdout-mode rerun`, missing replay path |
| Recompute holdout summaries | `rerun` | `--num-sims <N>`, `N > 0` | `--holdout-replay <path>` |
| Reuse TS-derived holdout summaries | `reuse` | none | `--holdout-replay <path>` |

Output scope is independent:
- `all`: include TS + holdout all-sims outputs
- `holdout`: holdout-only all-sims output

## 4. Current Command Mapping (Until CLI Rollout Lands)

Use this mapping to avoid ambiguity before canonical flags are fully implemented:

- `replay` intent:
  - Must include `--holdout-replay <path>`
  - Must not include `--holdout-rerun`
- `rerun` intent:
  - Must include `--holdout-rerun --num-sims <N>`
  - Must not include `--holdout-replay`
- `reuse` intent:
  - Omit both `--holdout-rerun` and `--holdout-replay`

For holdout-only output in current CLI:
- Add `--holdout-only`

## 5. Command Templates

### A. Replay

```bash
python3 scripts/run_opening_white12_analysis.py \
  --num-sims 200 \
  --holdout-replay data/analysis/opening_states/0003/initial_branch_analysis_all_sims_holdout.csv \
  --exclude-sample-ids 0001,0002,0010,0011,0012
```

### B. Rerun

```bash
python3 scripts/run_opening_white12_analysis.py \
  --holdout-rerun \
  --holdout-only \
  --budget 3000 \
  --num-sims 200 \
  --exclude-sample-ids 0001,0002,0010,0011,0012
```

### C. Reuse

```bash
python3 scripts/run_opening_white12_analysis.py \
  --budget 3000 \
  --num-sims 200 \
  --exclude-sample-ids 0001,0002,0010,0011,0012
```

## 6. Anti-Patterns (Do Not Run)

- Request says "replay" but command uses `--holdout-rerun`.
- `--holdout-replay <path>` combined with rerun intent.
- Rerun intent with `--num-sims 0`.
- Launching long runs without printing resolved mode, board scope, sims, budget, and replay path.

## 7. Mandatory Preflight Checklist

1. Intended mode is explicitly written: `replay`, `rerun`, or `reuse`.
2. Final command is pasted and reviewed before launch.
3. Board/sample scope is explicit (`--exclude-sample-ids` or equivalent).
4. Output path/log path is explicit.
5. Replay path is present if and only if mode is `replay`.
6. For rerun, confirm `--num-sims > 0`.

