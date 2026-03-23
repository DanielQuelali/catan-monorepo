# Hex Gambit - Technical Spec

Status: Active  
Owner: Engineering  
Last Updated: 2026-03-23

## 1. Purpose

Define the current implementation architecture, data contracts, deterministic decision logic, and runtime behavior for Hex Gambit.

## 2. Product Contract Alignment

This spec implements the active PRD at `docs/initial-placement-product-prd.md`:

1. WHITE-only opening-placement evaluation.
2. Deterministic 4-step per-board flow (`s1`, `e1`, `s2`, `e2`).
3. Session progression through all boards configured in `apps/hex-gambit/data/boards.json`.
4. No in-session replay of completed boards.
5. End-of-session output includes playstyle, rationale, and badge.
6. Local-only runtime with in-memory session state.

## 3. Repository Context

1. App runtime: `apps/hex-gambit`
2. Analysis producer: `crates/initial-branch-analysis`
3. Analysis orchestration script: `scripts/run_opening_white12_analysis.py`
4. Runtime analysis assets: `runtime-data/opening_states/`

## 4. Architecture

## 4.1 High-level components

1. Data producer
- `crates/initial-branch-analysis` emits per-board analysis CSV artifacts.

2. Runtime assets
- Board fixture payload: `apps/hex-gambit/data/boards.json`
- Analysis CSV assets: `runtime-data/opening_states/<board_id>/initial_branch_analysis_all_sims_holdout.csv(.gz)`

3. UI runtime
- `apps/hex-gambit/app.js` handles model build, state machine, legal-action gating, scoring, and rendering.

## 4.2 Runtime module boundaries

1. `index.html`
- static shell and viewport containers.

2. `app.js`
- payload load and normalization
- deterministic session/board state transitions
- CSV load and ranking derivation
- playstyle signal aggregation and summary rendering.

3. `styles.css`
- responsive visual system for intro, placement, board-result, summary, and error states.

## 5. Data Contracts

## 5.1 Board payload contract (`boards.json`)

Top-level required fields:

1. `meta`
2. `boardModel`
3. `boards`

`meta` fields used by runtime:

1. `currentColor` or `perspectiveColor` (defaults to `WHITE` if absent)
2. `sequenceKeys` (optional; falls back to default 4-step sequence)

`boardModel` fields:

1. `nodes[]` with `id`, `tile_coordinate`, `direction`
2. `edges[]` with `id` node pair, `tile_coordinate`, `direction`

`boards[]` fields:

1. `id`
2. `label`
3. `tiles[]` (resource/desert/port/water descriptors)
4. `basePlacedNodes[]`
5. `basePlacedEdges[]`
6. optional analysis overrides:
- `analysis_id` / `analysisId`
- `analysis_path` / `analysisPath`
- `analysis_root` / `analysisRoot`
7. optional `seedSelection`

Validation and behavior notes:

1. Board count is dynamic and equals `boards.length` (currently 8).
2. First board geometry (`boards[0]`) is used as board-model template for rendering.
3. Missing or invalid payload hard-fails load and enters error stage.

## 5.2 Analysis artifact contract

Per board, runtime attempts analysis sources in order:

1. explicit board analysis path (`analysis_path`/`analysisPath`) if provided
2. default root path:
- `./runtime-data/opening_states/<analysis_id>/initial_branch_analysis_all_sims_holdout.csv.gz`
- fallback `.csv`

Required CSV columns used by runtime:

1. `LEADER_SETTLEMENT`
2. `LEADER_ROAD`
3. `LEADER_SETTLEMENT2`
4. `LEADER_ROAD2`
5. `WIN_WHITE`

Optional but consumed when present:

1. `WIN_<COLOR>` columns for non-WHITE player win bars
2. `SIMS_RUN` for weighted aggregation
3. follower placement columns for board-result reveal rendering

## 6. Deterministic Domain Logic

## 6.1 Runtime stage state machine

Stages:

1. `loading`
2. `intro`
3. `placement`
4. `board_result`
5. `summary`
6. `error`

Rules:

1. App starts in `loading`, then transitions to `intro` only after payload + all board analyses load.
2. Any load/parse failure transitions to `error`.
3. `startSession()` resets deterministic session state and enters `placement`.

## 6.2 Per-board step progression

Step keys (default order):

1. `s1` settlement
2. `e1` road
3. `s2` settlement
4. `e2` road

Rules:

1. Only legal options for current step are selectable.
2. Invalid selections are ignored and do not mutate state.
3. Board result is computed only after the final step is selected.

## 6.3 Session progression

Rules:

1. Session iterates boards in payload order from index `0` to `boards.length - 1`.
2. Continue action advances forward only.
3. Completed board replay is not exposed.
4. Restart resets board index, step index, selections, signals, and results.

## 6.4 Board ranking and tie-breaks

1. CSV rows are aggregated per full sequence key (`s1|e1|s2|e2`).
2. Win percentages are weighted by `SIMS_RUN` when available; otherwise unweighted averaging is used.
3. Sequence ranking sort order:
- WHITE win percent descending
- canonical sequence tuple ascending (deterministic tie-break).
4. Rank display is `rank / N`.

## 6.5 Playstyle classification

Signals:

1. `rank`: chosen action follows top-ranked prefix.
2. `ows`: settlement action aligned with best continuation win.
3. `road`: road action aligned with best continuation win.

Signal aggregation:

1. One signal emitted per placement step across entire session.
2. Winner is highest count.
3. Deterministic tie precedence:
- `rank`
- `ows`
- `road`

Mapping:

1. `rank` -> `Top-Rank Absolutist`
2. `ows` -> `OWS Dev Card Specialist`
3. `road` -> `Road Network Architect`

## 7. UI Behavior Requirements

1. Intro copy must explain evaluation objective and session result outcome.
2. Placement screen must show step progress over total session steps.
3. Board-result screen must show board completion plus rank context.
4. Summary must show playstyle, rationale, badge, and per-board rank recap.
5. Restart controls must be available on board-result and summary screens.

## 8. Session and Storage Rules

1. Session state is runtime memory only.
2. No localStorage/server persistence contract is required.
3. Refreshing browser context resets session state.

## 9. Error Handling

1. Board payload load failure -> `error` stage with explicit message.
2. Analysis CSV load/decode failure for any board -> `error` stage.
3. Missing sequence result for completed board selection -> explicit error; no default 0% fallback.

## 10. Performance Expectations

1. Payload parse and geometry build occur once per app load.
2. Board analyses are loaded before intro state.
3. Legal-option derivation and board rendering remain interactive under local fixture sizes.

## 11. Testing Strategy

## 11.1 Contract checks

1. Validate `boards.json` shape and board-count assumptions for release bundles.
2. Validate analysis CSV presence for all configured board ids.
3. Validate deterministic rank ordering from a fixed CSV sample.

## 11.2 Runtime behavior checks

1. Full-session completion across current board set.
2. No completed-board replay path in-session.
3. Restart resets state and returns to initial placement board.
4. Error stage triggered for missing/invalid analysis file.

## 11.3 Regression checks

1. Verify stable playstyle output for identical decision paths.
2. Verify rank display format remains `rank / N`.
3. Verify summary always includes badge and rationale.

## 12. Documentation Artifacts

1. Product PRD: `docs/initial-placement-product-prd.md`
2. Analysis artifact contract: `docs/hex-gambit-analysis-artifact-spec.md`
3. Current-state index: `docs/README.md`
4. Legacy competitor docs (preserved):
- `docs/legacy/competitor/initial-placement-product-prd-competitor-legacy.md`
- `docs/legacy/competitor/initial-placement-technical-spec-competitor-legacy.md`
