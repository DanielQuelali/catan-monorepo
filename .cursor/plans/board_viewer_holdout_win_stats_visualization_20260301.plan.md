---
name: Board Viewer holdout win-stat visualization
overview: Show new leader win-condition holdout columns on top-5 holdout cards in opening-board-viewer, while preserving current layout/behavior when columns are absent.
todos:
  - id: detect-columns
    content: Detect whether WIN_<LEADER>_* win-condition columns are present in loaded holdout CSV
  - id: aggregate-stats
    content: Aggregate win-condition metrics per top-pick placement (same weighted/unweighted semantics as WIN_WHITE aggregation)
  - id: render-cards
    content: Render win-condition metric section in each top-5 card when available; keep current card layout when unavailable
  - id: wider-layout
    content: Add conditional wider layout only for enhanced cards; leave current layout untouched by default
  - id: validate
    content: Validate both modes (legacy CSV and new CSV) and confirm top-5 ranking/hover behavior remain unchanged
isProject: false
---

# Board Viewer Plan: visualize new holdout win-condition columns (Top 5)

## Goal

Update `apps/opening-board-viewer` so each of the existing **top 5** holdout picks can show the new `WIN_<LEADER>_*` columns (from holdout CSV), without breaking legacy datasets.

## Hard requirement

If win-condition columns are not present in the CSV, the viewer must preserve today’s layout and behavior (no layout expansion, no empty placeholders, no regressions).

## Scope

In scope:
- `apps/opening-board-viewer/app.js`
- `apps/opening-board-viewer/styles.css`
- Optional tiny text tweak in `apps/opening-board-viewer/index.html` if needed

Out of scope:
- Changing ranking logic beyond current top-5 selection
- Changing board hover preview behavior
- Backend/analysis generation changes

## Data contract and detection strategy

Expected optional columns (leader example `WHITE`):
- `WIN_WHITE_PCT_HAS_SETTLEMENT`
- `WIN_WHITE_AVG_SETTLEMENTS`
- `WIN_WHITE_AVG_CITIES`
- `WIN_WHITE_PCT_HAS_CITY`
- `WIN_WHITE_PCT_HAS_VP`
- `WIN_WHITE_AVG_VP_GIVEN_HAS`
- `WIN_WHITE_PCT_LA`
- `WIN_WHITE_PCT_LR`
- `WIN_WHITE_PCT_BOTH`
- `WIN_WHITE_PCT_PLAYED_MONOPOLY`
- `WIN_WHITE_PCT_PLAYED_YOP`
- `WIN_WHITE_PCT_PLAYED_ROAD_BUILDER`
- `WIN_WHITE_PCT_PLAYED_KNIGHTS`
- `WIN_WHITE_AVG_KNIGHTS_GIVEN_PLAYED`
- `WIN_WHITE_AVG_TURN_FIRST_CITY`
- `WIN_WHITE_AVG_TURN_FIRST_SETTLEMENT`

Detection:
- Infer leader from `LEADER_COLOR` (already present).
- Build dynamic metric keys with `WIN_${leaderColor}_...`.
- Mark row as “enhanced” only if required metric set exists and parses as finite numbers.
- At dataset level, enable enhanced rendering only when at least one usable row for current sample has all required keys.

## Implementation plan

### 1) Extend holdout aggregation model in `app.js`

Current flow:
- `buildHoldoutTopPicks(rows)` groups rows by leader placements, computes weighted WIN_WHITE, returns top 5.

Change:
- Keep current ranking behavior unchanged.
- Add optional `winStats` aggregation object per grouped pick.
- Use same weighting rule as existing win aggregation:
  - If `SIMS_RUN > 0`, weighted by `SIMS_RUN`
  - Else unweighted fallback by count
- Produce normalized display-ready values per pick for all 16 metrics.

Deliverable:
- `topPick.winStats` present when columns are available; absent otherwise.

### 2) Add dual render paths for holdout cards

Current:
- `createHoldoutCard(...)` renders fixed card with win %, sample size, S1/S2/roads.

Change:
- Split into:
  - `createHoldoutCardLegacy(...)` (existing markup unchanged)
  - `createHoldoutCardEnhanced(...)` (legacy content + new metrics section)
- In `renderHoldoutTopPicks(...)`, select card variant based on dataset enhancement flag and pick stats availability.
- Keep hover/focus preview wiring identical for both variants.

Enhanced card content:
- Compact grouped metric block under existing lines:
  - Build progression: has settlement/city, avg settlements/cities, first-turn stats
  - VP and awards: has VP, avg VP given has, LA/LR/Both
  - Dev usage: monopoly/YOP/road builder/knights, avg knights given played

### 3) Conditional wider layout in `styles.css`

Requirement: only widen when enhanced stats are shown.

Change:
- Keep existing `.wrap` and card styles as default (legacy unchanged).
- Add conditional class on root container (example: `.wrap.holdout-enhanced`).
- Under enhanced class:
  - Increase max width (e.g. `1200 -> 1500/1600`)
  - Adjust grid columns to give holdout panel more room
  - Add enhanced metric styles (`.holdout-stats-grid`, `.holdout-stat`, etc.)
- Ensure mobile media query still collapses to single column.

### 4) Runtime mode toggling

In `renderHoldoutTopPicks(...)`:
- Determine `isEnhancedSample`.
- Toggle class on `.wrap` accordingly:
  - Enhanced on: `wrap.classList.add("holdout-enhanced")`
  - Enhanced off: `wrap.classList.remove("holdout-enhanced")`

This guarantees legacy CSV keeps current layout.

## Backward compatibility guarantees

- If no win-condition columns exist:
  - Top-5 cards render exactly current content.
  - No new spacing/panels appear.
  - No width/layout class is applied.
- Existing ranking/order/tie-break remains unchanged.
- Existing hover preview remains unchanged.

## Validation plan

### A) Legacy CSV (no new columns)
- Use an older holdout file without `WIN_<LEADER>_*`.
- Confirm:
  - top-5 cards look unchanged
  - no enhanced class on wrapper
  - hover preview works

### B) New CSV (with columns)
- Use current holdout file containing new columns.
- Confirm:
  - each top-5 card shows win-stat section
  - values render with expected formatting (`%` and decimals)
  - wrapper widens only in this mode
  - hover preview still works

### C) Robustness
- Missing/partial metrics on a row:
  - row falls back to legacy rendering for that pick
  - app remains functional

## Suggested implementation order

1. Data detection + aggregation in `buildHoldoutTopPicks`
2. Card render split (legacy/enhanced)
3. Conditional wrapper class toggle
4. CSS for enhanced mode
5. Manual validation with both CSV types

