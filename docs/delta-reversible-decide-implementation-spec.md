# Delta-Reversible `decide` Implementation Spec

## Objective

Replace clone-based rollback in `FastValueFunctionPlayer::decide` with delta-based reversible apply/undo to reduce single-thread playout cost while preserving exact behavior.

Target file:

- `crates/fastcore/src/value_player.rs`

## Current State

Today `decide` does this per candidate action:

1. apply candidate to scratch state
2. evaluate value
3. rollback by `scratch_state.clone_from(baseline_state)` and reset road/army snapshots

This clone rollback is a hot-path cost multiplier when there are many candidate actions.

## Non-Negotiable Constraints

1. Exact same decisions for fixed seed and input state.
2. Exact same RNG consumption order.
3. No change to public behavior, rules, tie-breaks, or action ordering.
4. Keep code safe Rust only (`#![forbid(unsafe_code)]` remains honored).

## Design Summary

Introduce a dedicated reversible simulation apply path for `decide`:

1. apply candidate action with a `DecisionDelta` recorder
2. evaluate value
3. undo exactly from `DecisionDelta`

This replaces full-state clone rollback with compact change logs.

## API and Type Additions

### 1) New decision delta type

Add to `crates/fastcore/src/value_player.rs` (or a nearby private module):

```rust
struct DecisionDelta {
    state_delta: Delta,             // roads/buildings/resources/bank/turn/robber
    prev_road_state: RoadState,     // full snapshot; small Copy type
    prev_army_state: ArmyState,     // full snapshot; small Copy type
    prev_prompt: ActionPrompt,
    prev_turn_phase: TurnPhase,
    prev_active_player: PlayerId,
    prev_turn_player: PlayerId,
    prev_flags: DecisionFlagsSnapshot,
    prev_free_roads_available: u8,
    prev_trade: [u8; RESOURCE_COUNT * 2],
    prev_acceptees: [bool; PLAYER_COUNT],
    prev_trade_offering_player: PlayerId,
    prev_trade_offered_this_turn: bool,
    prev_last_initial_settlement: [NodeId; PLAYER_COUNT],
}
```

`DecisionFlagsSnapshot` is a compact struct for:

- `is_initial_build_phase`
- `is_discarding`
- `is_moving_robber`
- `is_road_building`
- `is_resolving_trade`
- `has_rolled`
- `has_played_dev`
- `dev_owned_at_start`

Reason: `Delta` currently tracks core board/resource mutations but not all control fields touched by value actions.

### 2) New reversible apply function

Add private function in `value_player.rs`:

```rust
fn apply_value_action_reversible(
    board: &Board,
    state: &mut State,
    road_state: &mut RoadState,
    army_state: &mut ArmyState,
    action: &ValueAction,
    rng: &mut impl RngCore,
    delta: &mut DecisionDelta,
)
```

Requirements:

1. Clear/reset `delta` at start.
2. Capture all non-`Delta` fields listed above before mutation.
3. Reuse existing action legality/apply logic semantics exactly.
4. Use a `Delta` recording path for state/resource/road/building/bank/turn/robber changes.

### 3) Undo function

Add:

```rust
fn undo_value_action_reversible(
    state: &mut State,
    road_state: &mut RoadState,
    army_state: &mut ArmyState,
    delta: &DecisionDelta,
)
```

Undo order:

1. `state.undo(&delta.state_delta)` for structural/resource deltas.
2. restore full `road_state` and `army_state` snapshots.
3. restore all captured control fields exactly.

## Decide-Loop Integration

In `FastValueFunctionPlayer::decide`:

1. Build sorted candidate list exactly as current code.
2. For each candidate:
   1. call `apply_value_action_reversible(...)`
   2. evaluate value
   3. `undo_value_action_reversible(...)`
3. Keep comparison logic identical (`>` only for value winner; stable action order from sorting).

Important: no candidate-specific RNG branching changes beyond what current apply already does.

## Determinism Rules

Must remain true:

1. For fixed state + seed, selected action equals current implementation.
2. For fixed seeds, full playout winners/turn counts equal baseline.
3. Policy logs (`simulate_policy_log`) byte-identical for fixed seeds.

Implementation rule:

- Do not reorder candidates.
- Do not short-circuit candidate evaluation earlier than current logic.
- Do not skip RNG calls in candidate apply paths.

## Edge Cases to Handle Explicitly

1. `MoveRobber` with random steal selection.
2. `Discard(None)` random discard generation.
3. `BuyDevCard` mutations (`dev_deck`, `dev_owned_at_start`, `dev_cards_in_hand`).
4. `PlayRoadBuilding` free-road flags and counters.
5. Trade prompt fields (`current_trade`, `acceptees`, `trade_offering_player`).
6. Initial placement prompt transitions and `last_initial_settlement`.

## Test Plan (Required)

### Unit tests (new)

Add tests in `crates/fastcore/tests/value_player.rs`:

1. `reversible_apply_undo_is_identity_for_each_action_kind`
2. `reversible_apply_matches_kernel_apply_result_before_undo`
3. `reversible_path_keeps_rng_consumption_identical`

### Differential tests (required)

1. Old `decide` implementation vs new reversible `decide` on fixed fixtures and seeds:
   - selected action equality
2. Full playout differential:
   - winners
   - turns
   - illegal actions
3. Policy-log equality on fixed seed set.

### Existing test suites that must pass

1. `cargo test -p fastcore --all-features`
2. `cargo test -p initial-branch-analysis --all-features`
3. `evals/single_thread/verify_against_golden.sh` against locked golden artifacts

## Performance Validation

Measure in release mode:

1. `target/release/bench_value_state --sims 1000 --seed 1 --max-turns 1000`
2. Run at least 5 repetitions, report median games/sec.
3. Compare against pre-change median.

Acceptance threshold:

- minimum +8% median single-thread improvement from this change alone.
- no correctness drift allowed.

## Rollout Steps

1. Implement reversible apply/undo in `value_player.rs` behind a local constant:
   - `const USE_REVERSIBLE_DECIDE: bool = true;`
2. Keep temporary fallback path for one PR for easy A/B validation.
3. Run full differential and harness checks.
4. Remove fallback once parity is proven.

## Failure / Revert Criteria

Immediate revert if any of:

1. Any deterministic mismatch in harness artifacts.
2. Any action-choice diff in fixed-seed decide tests.
3. Median perf gain below threshold after noise-controlled measurements.

## Handoff Checklist

1. Implementation references this spec section-by-section in PR notes.
2. PR includes:
   - before/after benchmark table
   - deterministic diff evidence
   - list of fields captured in `DecisionDelta`
3. Verifier reruns all required commands independently and signs off.

## Implementation Note (2026-03-01)

During implementation, a fully generic reversible path (`apply kernel` + snapshot all control fields + diff all core arrays) was functionally correct but slower than the prior clone rollback in single-thread hot loops.

Observed cause:

1. per-candidate snapshot/diff overhead across many arrays dominated the savings from avoiding `clone_from`
2. the hot decision workload is heavily skewed toward build-action candidate evaluation

Adopted approach:

1. keep generic reversible apply/undo as correctness fallback
2. add lightweight reversible snapshots for hot build actions (`BuildSettlement`, `BuildCity`, `BuildRoad`) that capture only required fields
3. route `decide` through lightweight path when eligible, generic reversible fallback otherwise

Result:

1. deterministic behavior preserved (tests and deterministic regression checks)
2. single-thread bench recovered and improved versus the slower generic-only reversible path
