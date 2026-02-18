# Initial Placement Trainer - Technical Spec

Status: Draft  
Owner: Engineering  
Last Updated: 2026-02-16

## 1. Purpose

Define the implementation architecture, data contracts, and validation plan for the Initial Placement Trainer so the system is deterministic, maintainable, and agent-legible.

## 2. Repository Context

Current monorepo layout:

1. `apps/catanatron-ui`
2. `crates/fastcore`
3. `crates/initial-branch-analysis`
4. `data/`
5. `data/analysis/`

## 2.1 Product Contract Alignment (MVP)

This technical spec implements the current product PRD contract for MVP:

1. WHITE-only initial placement training flow (4th player), fixed 4-step sequence.
2. Exactly 2 curated rounds/levels in the MVP manifest.
3. Local-only mode for MVP (no account/sync/analytics dependency).
4. Round scoring centered on global rank display as `rank / N`.
5. Session summary metric centered on average global rank across the 2 rounds.

## 3. Legacy Baseline and Required Corrections

Legacy source of truth reviewed:

1. `catan-sim/settlement-placement-app/index.html`
2. `catan-sim/settlement-placement-app/white-levels.html`
3. `catan-sim/settlement-placement-app/scripts/serialize_white_levels.js`

Corrections required:

1. Replace monolithic inline app with modular UI implementation.
2. Remove manual upload UX in production.
3. Fix holdout ingestion semantics: `NUM_SIMS == 0` is valid.
4. Enforce schemas through tests, not runtime best effort only.

## 4. Architecture

## 4.1 High-Level Components

1. Data Producer
- `crates/initial-branch-analysis` generates analysis artifacts under `data/analysis/`.

2. Data Preparation Layer
- Optional converter/indexer scripts produce UI-friendly level artifacts (JSON/BIN).

3. Training UI
- `apps/catanatron-ui` consumes curated level artifacts and renders guided drills.

## 4.2 Module Boundaries in UI

Suggested directories under `apps/catanatron-ui/src/`:

1. `training/data/`
- manifest loader
- schema validators
- binary decoder
- adapters to UI view model

2. `training/domain/`
- step state machine
- ranking logic
- tie-breaking logic
- score aggregation

3. `training/board/`
- board projection and coordinate utilities
- action affordance helpers

4. `training/ui/`
- step console
- options list
- result card
- session summary

5. `training/tests/`
- fixtures
- integration tests
- contract tests

## 4.3 Agent-First Development Flow (Required)

The training surface must be implemented and operated so coding agents can reproduce bugs, apply fixes, and validate behavior directly.

### 4.3.1 Per-Worktree Bootability

Requirements:

1. App must be bootable from any git worktree without manual setup changes.
2. Each worktree must be able to run an isolated app instance per change.
3. Runtime ports and temp outputs must be namespace-safe by worktree.
4. One-command local start must be available from repo root for agent use.

Implementation guidance:

1. Derive a stable worktree id from branch/worktree path.
2. Map worktree id to deterministic dev-server ports.
3. Keep generated runtime artifacts out of tracked source paths.

### 4.3.2 Chrome DevTools Protocol (CDP) Integration

Requirements:

1. Agent runtime must support CDP-driven browser automation for this app.
2. Agent must be able to:
- select target tab/page
- clear console and runtime errors
- navigate to training routes
- capture DOM snapshots (before/after)
- capture screenshots (before/after)
- inspect runtime events during interaction

3. CDP operations must be scriptable and repeatable for regression checks.

### 4.3.3 Agent Skills and Validation Primitives

Required skill capabilities:

1. Navigation skill
- open route
- execute deterministic UI path

2. Snapshot skill
- capture structured DOM snapshot
- diff snapshot against baseline

3. Screenshot skill
- capture viewport evidence for key states

4. Console/runtime skill
- collect console errors/warnings during interaction
- fail validation on unexpected runtime errors

### 4.3.4 Standard Agent Validation Loop

For UI bugs and regressions, use this loop until clean:

1. Select target + clear console.
2. Capture `BEFORE` DOM snapshot and screenshot.
3. Trigger deterministic UI interaction path.
4. Capture runtime events during interaction.
5. Capture `AFTER` DOM snapshot and screenshot.
6. Apply fix + restart app instance.
7. Re-run validation loop.

Exit criterion:

1. No unexpected console/runtime errors.
2. Snapshot and screenshot assertions pass.
3. Behavioral acceptance checks pass for the targeted path.

## 5. Data Contracts

## 5.1 Manifest Contract

Example source: `white_levels_manifest.json`.

Required fields:

1. `schemaVersion: number`
2. `title: string`
3. `levels: array`
4. `levels[].id: string`
5. `levels[].title: string`
6. `levels[].dataUrl: string`

Validation rules:

1. `levels` must be non-empty.
2. `id` values must be unique.
3. `dataUrl` must resolve within allowed repo-backed data roots.
4. MVP profile requires exactly 2 levels in the manifest.

## 5.2 Level Metadata Contract

Example legacy format: `white12_bin_v1`.

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

1. `format` must be recognized.
2. `fieldsPerRow` must match decoder expectation.
3. `sequenceCount` must match binary payload length.
4. Invalid payload must hard-fail load for that level.

## 5.3 Binary Sequence Contract (white12_bin_v1)

Per-row values (uint16 little-endian):

1. `settlement1`
2. `road1_a`
3. `road1_b`
4. `settlement2`
5. `road2_a`
6. `road2_b`
7. `winWhiteScaled`

Derived field:

1. `winWhite = winWhiteScaled / winScale`

## 5.4 CSV Analysis Contract

When consuming analysis CSV data directly or in conversion scripts:

1. `NUM_SIMS == 0` is valid and must not be dropped.
2. Numeric parse failures are errors for required scoring fields.
3. Canonical edge key format is `min-max`.
4. Tie-breaking must be deterministic and documented.

## 6. Deterministic Domain Logic

## 6.1 Step State Machine

States:

1. `choose_settlement_1`
2. `choose_road_1`
3. `choose_settlement_2`
4. `choose_road_2`
5. `level_complete`

Transition rules:

1. Each step accepts only legal options derived from current prefix.
2. Invalid selection has no state mutation.
3. Completion only occurs if full sequence key exists.

## 6.2 Ranking and Tie-Breaks

Per-step options are ranked by:

1. Highest `best_continuation_win_white` descending.
2. Settlement step tie-break: settlement id ascending.
3. Road step tie-break: canonical edge ascending.

Global sequence rank per level:

1. Sort all valid full sequences by `winWhite` descending.
2. Deterministic tie-break by canonical sequence tuple ascending.
3. Global rank is 1-based ordinal index in this sorted list.
4. Display contract is `rank / N`, where `N` is total valid full sequences in the level.

## 7. UI Behavior Requirements

1. Production mode must not render file upload controls.
2. Current step label always visible.
3. Available actions clearly highlighted.
4. Non-available nodes/edges are non-interactive.
5. Option list displays action, best win%, rank.
6. Completion card shows selected win% and global rank (`rank / N`).
7. Session summary shows average global rank across the 2 MVP rounds.

## 8. Error Handling

1. Manifest load failure: show recoverable user-safe error and retry action.
2. Level payload mismatch: show level-specific load error, do not crash whole app.
3. Board mismatch: prevent interaction until resolved.
4. Missing sequence on completion: show explicit "no precomputed result" message.

## 9. Performance Requirements

1. Parse/validate manifest once per session.
2. Lazy-load per-level metadata and binary payload.
3. Keep per-step option derivation under 100 ms at p95 on fixture levels.
4. Avoid full-board re-layout unless board changes.

## 10. Testing Strategy

## 10.1 Contract Tests

1. Manifest schema validation.
2. Level metadata schema validation.
3. Binary payload length and row decoding checks.
4. CSV converter tests including `NUM_SIMS == 0` preservation.

## 10.2 Domain Unit Tests

1. State machine transition coverage.
2. Prefix filtering correctness for each step.
3. Ranking and tie-break determinism.
4. Global rank computation consistency.

## 10.3 Integration Tests

1. End-to-end WHITE level flow on fixture dataset.
2. UI displays exact expected `rank / N` and win% for selected known sequence.
3. Session summary computes average global rank across exactly 2 rounds.

## 10.4 Regression Fixtures

Fixture set should include:

1. Normal dataset with positive `NUM_SIMS`.
2. Holdout dataset with `NUM_SIMS == 0` rows.
3. Corrupt binary payload fixture for error-path validation.

## 10.5 Agent-Driven UI Validation

1. Provide deterministic UI path scripts for core flows:
- level load
- step transitions
- level completion
- summary rendering

2. For each scripted path, capture:
- console/runtime log report
- BEFORE/AFTER DOM snapshots
- BEFORE/AFTER screenshots

3. Validation artifacts should be emitted in a non-tracked temp/report path and attached in CI logs when checks fail.

## 11. CI Gates

1. Contract tests must pass.
2. Domain and integration tests must pass.
3. Lint/typecheck must pass.
4. No production build may contain upload controls for trainer mode.
5. Agent-driven CDP validation suite must pass for critical UI paths.

## 12. Migration Plan

1. Preserve legacy behavior parity for WHITE 4-step flow and ranking semantics.
2. Replace legacy standalone HTML with integrated module in `apps/catanatron-ui`.
3. Move stable training data into `data/` and `data/analysis/`.
4. Keep legacy artifacts only as migration fixtures, not runtime product dependencies.

## 13. Rollout Plan

1. Phase 1
- Implement data contracts, parser modules, and domain state machine.

2. Phase 2
- Integrate board + step console + option ranking UI.

3. Phase 3
- Add test harness, CI gates, and performance checks.

4. Phase 4
- Deprecate legacy trainer entry points and document migration completion.

## 14. Security and Integrity Notes

1. Only allow trusted data roots for manifest/level URLs.
2. Treat all loaded data as untrusted until validated.
3. Never execute dynamic code from datasets.

## 15. Open Technical Decisions

1. Keep binary format as canonical, or introduce normalized JSON index for debugging and tests?
2. Store board snapshots per level, or derive from common base plus diffs?
3. Should debug import mode exist behind non-production feature flag?

## 16. Documentation Artifacts

1. Product narrative and requirements live in `docs/initial-placement-product-prd.md`.
2. The MVP user flow diagram is embedded directly in `docs/initial-placement-product-prd.md` under the Core Training Loop section.
