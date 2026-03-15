---
name: Win condition summary statistics
overview: Add leader-conditioned win-condition summary columns to the holdout CSV (e.g. for WHITE in white12): point composition, dev card usage, LA/LR/both, and turn-to-first-settlement/city.
todos:
  - id: winner-breakdown-struct
    content: Add LeaderWinBreakdown struct and compute from state at game end (buildings, cities, VP cards, LA, LR, dev plays from state.dev_cards_played)
  - id: track-first-turn
    content: Track first_settlement_turn and first_city_turn per player during sim loop (on BuildSettlement/BuildCity)
  - id: playout-result-aggregate
    content: Extend PlayoutResult with breakdown; extend PlayoutAggregate with leader-only counts/sums; pass leader PlayerId through run_playouts/summarize_playouts
  - id: summary-csv
    content: Add PlayoutSummary leader win-condition fields and holdout CSV columns for leader only
  - id: docs
    content: Document new holdout columns in artifact spec
isProject: false
---

# Win condition summary statistics (leader only)

## Scope

Add columns to the **holdout CSV only**, for the **leader** color (e.g. WHITE when using white12). All new stats are **conditioned on the leader winning**.

## Column list (exact)

All conditioned on leader (e.g. WHITE) winning:

- **Pct wins with ≥1 settlement built** — % of leader wins where leader built at least one settlement (beyond initial 2)
- **Avg settlements built** — Avg number of settlements built (excluding initial 2), over leader wins
- **Avg cities built** — Avg number of cities built, over leader wins
- **Pct wins with ≥1 city built** — % of leader wins where leader built at least one city
- **Pct wins with VP cards** — % of leader wins where leader had at least one VP dev card (in hand at game end)
- **Avg VP cards (given ≥1)** — Avg number of VP cards when leader had at least one, over those leader wins
- **Pct wins with Largest Army** — % of leader wins where leader had LA
- **Pct wins with Longest Road** — % of leader wins where leader had LR
- **Pct wins with Both** — % of leader wins where leader had both LA and LR
- **Pct wins played Monopoly** — % of leader wins where leader played Monopoly at least once
- **Pct wins played Year of Plenty** — % of leader wins where leader played YOP at least once
- **Pct wins played Road Builder** — % of leader wins where leader played Road Building at least once
- **Pct wins played Knights** — % of leader wins where leader played at least one Knight
- **Avg Knights played (given ≥1)** — Avg number of Knights played when at least one was played, over those leader wins
- **Avg turns until first city built** — Among leader wins where they built ≥1 city, avg turn of first city
- **Avg turns until first settlement built** — Among leader wins, avg turn of first settlement built (beyond initial)

Suggested CSV header names (leader e.g. WHITE): `WIN_WHITE_PCT_HAS_SETTLEMENT`, `WIN_WHITE_AVG_SETTLEMENTS`, `WIN_WHITE_AVG_CITIES`, `WIN_WHITE_PCT_HAS_CITY`, `WIN_WHITE_PCT_HAS_VP`, `WIN_WHITE_AVG_VP_GIVEN_HAS`, `WIN_WHITE_PCT_LA`, `WIN_WHITE_PCT_LR`, `WIN_WHITE_PCT_BOTH`, `WIN_WHITE_PCT_PLAYED_MONOPOLY`, `WIN_WHITE_PCT_PLAYED_YOP`, `WIN_WHITE_PCT_PLAYED_ROAD_BUILDER`, `WIN_WHITE_PCT_PLAYED_KNIGHTS`, `WIN_WHITE_AVG_KNIGHTS_GIVEN_PLAYED`, `WIN_WHITE_AVG_TURN_FIRST_CITY`, `WIN_WHITE_AVG_TURN_FIRST_SETTLEMENT`.

## Data sources

- **At game end from state:** Settlements/cities from `node_owner`/`node_level` for winner. Settlements built = (settlements + cities) − 2; cities = count City. VP cards: `state.dev_cards_in_hand[winner][VictoryPoint]`. LA/LR: `army_state.owner()`, `road_state.owner()`. Played Monopoly/YOP/RoadBuilder/Knights: `state.dev_cards_played[winner][card]` (> 0 or count).
- **Track during sim:** Per player `first_turn_built_settlement: Option<u32>`, `first_turn_built_city: Option<u32>`. On BuildSettlement: if that player’s first is None, set to current turn. On BuildCity: same.

## Implementation outline

### 1. Leader win breakdown struct

- Add struct with: settlements_built, cities_built, vp_cards, had_lr, had_la, played_monopoly, played_yop, played_road_builder, knights_played, first_turn_settlement, first_turn_city. Fill from state + road_state + army_state + turn trackers. Use `fastcore::types::DevCard` indices for dev_cards_played.
- In `simulate_from_state_with_scratch`: maintain first_settlement_turn and first_city_turn per player; after apply_value_action_kernel, if action is BuildSettlement/BuildCity set turn for that player when None. When building PlayoutResult, if winner is Some, fill breakdown for that winner.

### 2. Pass leader into aggregation

- `run_playouts` and `summarize_playouts` take optional **leader** (PlayerId). Resolve from sort_color/leader color at call sites. Only aggregate leader breakdown when `result.winner == leader`.

### 3. PlayoutAggregate (leader-only counts/sums)

- Add leader-only fields: leader_wins, leader_wins_has_settlement, sum_settlements, sum_cities, leader_wins_has_city, leader_wins_has_vp, sum_vp_cards, leader_wins_has_vp_count, leader_wins_la, leader_wins_lr, leader_wins_both, leader_wins_played_monopoly, leader_wins_played_yop, leader_wins_played_road_builder, leader_wins_played_knights, sum_knights_played, leader_wins_played_knights_count, sum_turn_first_city, leader_wins_has_city_count, sum_turn_first_settlement, leader_wins_has_settlement_count. In update(), only when winner == leader and breakdown is Some. Implement merge().

### 4. PlayoutSummary and holdout CSV

- PlayoutSummary gets one set of leader win-condition stats (16 values). When leader is None or leader_wins == 0, use 0 or 0.0. For “avg given ≥1” use 0 when denominator 0.
- all_sims_holdout_headers and all_sims_row: append 16 columns only for the leader color (e.g. WIN_WHITE_*). Runner passes leader color when building rows.

### 5. Docs

- Update docs/hex-gambit-analysis-artifact-spec.md: list new holdout columns as optional; state they are conditioned on leader winning.
