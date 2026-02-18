# Hex Gambit - Technical Spec

Status: Draft  
Owner: Engineering  
Last Updated: 2026-02-16

## 1. Purpose

Define implementation architecture, data contracts, and deterministic profiling logic for Hex Gambit.

## 2. Legacy Preservation

The competitor-focused documents are preserved unchanged at:

1. `docs/legacy/competitor/initial-placement-product-prd-competitor-legacy.md`
2. `docs/legacy/competitor/initial-placement-technical-spec-competitor-legacy.md`

## 3. MVP Product Contract Alignment

This spec implements the active PRD at `docs/initial-placement-product-prd.md`:

1. WHITE-only initial placement flow (4th player), fixed 4-step board sequence.
2. Exactly 2 boards per session in MVP.
3. No single-board retry after board completion.
4. Full-session restart is allowed.
5. End-of-session output includes playstyle, explanation, and badge.
6. Local-only runtime with no state persistence across web sessions.

## 4. Repository Context

1. `apps/hex-gambit`
2. `crates/fastcore`
3. `crates/initial-branch-analysis`
4. `data/`
5. `data/analysis/`

## 5. Architecture

## 5.1 High-Level Components

1. Data producer
- `crates/initial-branch-analysis` produces analysis artifacts.

2. Data preparation
- Converter/indexer emits board artifacts consumable by UI.

3. Training UI
- `apps/hex-gambit` renders session flow, board flow, and playstyle results.

## 5.2 UI Module Boundaries

Suggested modules under `apps/hex-gambit/`:

1. `index.html`
- entry shell and static layout container

2. `app.js`
- session state machine
- board progression logic
- deterministic playstyle classifier
- summary computation and rendering

3. `styles.css`
- visual system and responsive behavior for main, board, and summary screens

4. `README.md`
- local run instructions and product notes

## 6. Data Contracts

## 6.1 Course Manifest Contract

Required fields:

1. `schemaVersion: number`
2. `courseId: string`
3. `title: string`
4. `boards: array`
5. `boards[].id: string`
6. `boards[].title: string`
7. `boards[].dataUrl: string`

Validation rules:

1. `boards` must contain exactly 2 entries in MVP.
2. `boards[].id` must be unique.
3. `dataUrl` must resolve within approved local data roots.

## 6.2 Board Metadata Contract

Required fields:

1. `schemaVersion: number`
2. `format: string`
3. `id: string`
4. `title: string`
5. `boardStateUrl: string`
6. `encoding: string`
7. `fieldsPerRow: number`
8. `sequenceCount: number`
9. `winScale: number`
10. `dataBinUrl: string`

Validation rules:

1. `format` must be recognized by decoder.
2. `fieldsPerRow` must match decoder expectation.
3. `sequenceCount` must match payload length.
4. Invalid board payload hard-fails that board load.

## 6.3 Binary Sequence Contract

Per row values (uint16 little-endian):

1. `settlement1`
2. `road1_a`
3. `road1_b`
4. `settlement2`
5. `road2_a`
6. `road2_b`
7. `winWhiteScaled`

Derived:

1. `winWhite = winWhiteScaled / winScale`

## 7. Deterministic Domain Logic

## 7.1 Board Step State Machine

States:

1. `choose_settlement_1`
2. `choose_road_1`
3. `choose_settlement_2`
4. `choose_road_2`
5. `board_complete`

Rules:

1. Each step accepts only legal options from current prefix.
2. Invalid selections do not mutate state.
3. Completion occurs only when a full valid sequence exists.

## 7.2 Course State Machine

States:

1. `course_ready`
2. `board_1_active`
3. `board_1_complete_locked`
4. `board_2_active`
5. `course_complete`

Rules:

1. After board 1 completes, transition only to board 2 (no board 1 replay path).
2. After board 2 completes, transition to `course_complete`.
3. `restart_course` is allowed from any post-start state and resets to `board_1_active`.

## 7.3 Ranking and Tie-Breaks

Per-step option ordering:

1. `best_continuation_win_white` descending.
2. Settlement tie-break: settlement id ascending.
3. Road tie-break: canonical edge ascending.

Global sequence rank per board:

1. Sort valid full sequences by `winWhite` descending.
2. Tie-break by canonical sequence tuple ascending.
3. Rank is 1-based ordinal position.
4. Display format is `rank / N`.

## 7.4 Playstyle Classifier

Target playstyles:

1. `OWS Dev Card Specialist`
2. `Road Network Architect`
3. `Top-Rank Absolutist`

Signal definitions:

1. `picked_top_ows_devcard_signal`
- true when chosen action best preserves Ore/Wheat/Sheep access and development-card tempo.

2. `picked_top_road_expansion`
- true on road steps when chosen road edge matches top road-expansion heuristic.

3. `picked_rank1_prefix`
- true when chosen action remains consistent with the current board's rank-1 global sequence prefix.

Scoring:

1. `OWS Dev Card Specialist` score: count of `picked_top_ows_devcard_signal`.
2. `Road Network Architect` score: count of `picked_top_road_expansion` on road steps.
3. `Top-Rank Absolutist` score: count of `picked_rank1_prefix`.

Winner selection:

1. Highest total score wins.
2. Ties resolve deterministically in this precedence order:
- `Top-Rank Absolutist`
- `OWS Dev Card Specialist`
- `Road Network Architect`

Explanation output:

1. Include winner playstyle name.
2. Include top 1-2 supporting behavior statements from observed signals.
3. Include concise confidence phrase derived from winner margin.

## 8. Badge Mapping Contract

1. `OWS Dev Card Specialist` -> `badge_ows_devcard_specialist`
2. `Road Network Architect` -> `badge_road_network_architect`
3. `Top-Rank Absolutist` -> `badge_top_rank_absolutist`

Rules:

1. Exactly one badge is awarded per completed session.
2. Badge is shown in result UI immediately after playstyle assignment.
3. MVP does not persist badge inventory across web sessions.

## 9. UI Behavior Requirements

1. Session progress must be explicit (`Board 1/2`, `Board 2/2`).
2. Completed board UI is locked from replay in current run.
3. Full-session restart action is always available after first board starts.
4. Placement interaction is board-native: click legal nodes for settlements and legal edges for roads.
5. Board result shows selected win% and `rank / N`.
6. Session result shows average board rank summary, playstyle, explanation, and badge.
7. Product mode must not render upload/import controls.

## 10. Session and Storage Rules

1. Runtime state is in-memory only for MVP.
2. No cross-session persistence is allowed.
3. New web session starts from empty session/playstyle state.

## 11. Error Handling

1. Course manifest failure: block course start with local-only safe error.
2. Board payload failure: block affected board and offer full-session restart.
3. Missing completion sequence: show explicit local processing error for that board.

## 12. Performance Requirements

1. Parse/validate course manifest once per session.
2. Lazy-load board payload per board.
3. Keep per-step option derivation under 100 ms p95 on fixture data.
4. Keep playstyle assignment under 20 ms p95 per session.

## 13. Testing Strategy

## 13.1 Contract Tests

1. Course manifest schema and exact 2-board MVP constraint.
2. Board metadata and binary payload validation.
3. Feature derivation tests for playstyle signals.

## 13.2 Domain Unit Tests

1. Board state machine transition coverage.
2. Course state machine coverage with board lock semantics.
3. Rank computation determinism.
4. Classifier determinism and tie precedence behavior.

## 13.3 Integration Tests

1. End-to-end 2-board session completion.
2. Verify no single-board retry action is possible after board completion.
3. Verify full-session restart resets board and playstyle state.
4. Verify result screen includes playstyle, explanation, and badge.

## 14. CI Gates

1. Contract tests pass.
2. Domain and integration tests pass.
3. Lint/typecheck pass.
4. Production build contains no upload controls for trainer mode.

## 15. Documentation Artifacts

1. Product PRD and embedded flow diagram:
- `docs/initial-placement-product-prd.md`
2. Legacy competitor docs:
- `docs/legacy/competitor/initial-placement-product-prd-competitor-legacy.md`
- `docs/legacy/competitor/initial-placement-technical-spec-competitor-legacy.md`
