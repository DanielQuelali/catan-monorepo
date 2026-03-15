# Hex Gambit

Standalone web app for board-based playstyle evaluation.

## Run locally

From repo root (recommended):

```bash
./run-hex-gambit.sh
```

Open: `http://localhost:8080`

Optional custom port:

```bash
HEX_GAMBIT_PORT=8090 ./run-hex-gambit.sh
```

## Notes

- This app is intentionally separate from `apps/catanatron-ui`.
- The root launcher kills any process already using the assigned port before start.
- Main screen explicitly tells users their opening placements are evaluated and they will receive a playstyle result.
- Players click legal settlement and road spots directly on the Catan board for both boards.
- Board layouts are loaded from `apps/hex-gambit/data/boards.json` (opening-state fixtures).
- Board result win rates and global ranks are loaded from tracked runtime assets under `runtime-data/opening_states/<board_id>/initial_branch_analysis_all_sims_holdout.csv` (surfaced at `apps/hex-gambit/runtime-data/...` via symlink).
