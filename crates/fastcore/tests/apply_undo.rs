use fastcore::board::Board;
use fastcore::delta::Delta;
use fastcore::rules;
use fastcore::state::State;
use fastcore::types::{BuildingLevel, EdgeId, NodeId, PlayerId, Resource, PLAYER_COUNT};
use proptest::prelude::*;

#[derive(Copy, Clone, Debug)]
enum TestAction {
    BuildRoad(EdgeId),
    BuildSettlement(NodeId),
    BuildCity(NodeId),
}

fn seed_state_for_actions() -> State {
    let mut state = State::new();
    for player in 0..PLAYER_COUNT {
        for resource in Resource::ALL {
            state.player_resources[player][resource.as_index()] = 10;
        }
    }
    for resource in Resource::ALL {
        state.bank_resources[resource.as_index()] = 10;
    }

    let mut delta = Delta::default();
    state.set_building(0, 0, BuildingLevel::Settlement, &mut delta);
    state.set_road_owner(1, 0, &mut delta);
    state.set_road_owner(9, 0, &mut delta);

    state
}

fn collect_legal_actions(board: &Board, state: &State, player: PlayerId) -> Vec<TestAction> {
    let mut actions = Vec::new();
    for node in 0..fastcore::NODE_COUNT {
        let node_id = node as NodeId;
        if rules::is_legal_build_city(board, state, player, node_id) {
            actions.push(TestAction::BuildCity(node_id));
        }
    }
    for node in 0..fastcore::NODE_COUNT {
        let node_id = node as NodeId;
        if rules::is_legal_build_settlement(board, state, player, node_id) {
            actions.push(TestAction::BuildSettlement(node_id));
        }
    }
    for edge in 0..fastcore::EDGE_COUNT {
        let edge_id = edge as EdgeId;
        if rules::is_legal_build_road(board, state, player, edge_id) {
            actions.push(TestAction::BuildRoad(edge_id));
        }
    }
    actions
}

fn apply_action_for_test(
    state: &mut State,
    delta: &mut Delta,
    player: PlayerId,
    action: TestAction,
) {
    delta.reset();
    match action {
        TestAction::BuildRoad(edge) => {
            for resource in [Resource::Brick, Resource::Lumber] {
                state.adjust_resource(player, resource, -1, delta);
                state.adjust_bank(resource, 1, delta);
            }
            state.set_road_owner(edge, player, delta);
        }
        TestAction::BuildSettlement(node) => {
            for resource in [
                Resource::Brick,
                Resource::Lumber,
                Resource::Grain,
                Resource::Wool,
            ] {
                state.adjust_resource(player, resource, -1, delta);
                state.adjust_bank(resource, 1, delta);
            }
            state.set_building(node, player, BuildingLevel::Settlement, delta);
        }
        TestAction::BuildCity(node) => {
            for _ in 0..2 {
                state.adjust_resource(player, Resource::Grain, -1, delta);
                state.adjust_bank(Resource::Grain, 1, delta);
            }
            for _ in 0..3 {
                state.adjust_resource(player, Resource::Ore, -1, delta);
                state.adjust_bank(Resource::Ore, 1, delta);
            }
            state.set_building(node, player, BuildingLevel::City, delta);
        }
    }
}

#[test]
fn apply_and_undo_returns_to_original_state() {
    let mut state = seed_state_for_actions();
    let before = state.clone();
    let mut delta = Delta::default();

    state.set_road_owner(0, 0, &mut delta);
    state.undo(&delta);

    assert_eq!(state, before);
}

proptest! {
    #[test]
    fn apply_and_undo_is_identity_for_random_legal_action(index in 0usize..64) {
        let board = Board::standard();
        let mut state = seed_state_for_actions();
        let actions = collect_legal_actions(board, &state, 0);
        prop_assume!(!actions.is_empty());
        let action = actions[index % actions.len()];
        let before = state.clone();
        let mut delta = Delta::default();

        apply_action_for_test(&mut state, &mut delta, 0, action);
        state.undo(&delta);

        prop_assert_eq!(state, before);
    }
}

#[test]
fn resources_and_bank_never_negative_during_apply_and_undo() {
    let board = Board::standard();
    let mut state = seed_state_for_actions();
    let initial_player = state.player_resources[0];
    let initial_bank = state.bank_resources;
    let actions = collect_legal_actions(board, &state, 0);
    assert!(!actions.is_empty());
    let action = actions[0];

    let mut delta = Delta::default();
    apply_action_for_test(&mut state, &mut delta, 0, action);

    for resource in Resource::ALL {
        let idx = resource.as_index();
        assert!(state.player_resources[0][idx] <= initial_player[idx]);
        assert!(state.bank_resources[idx] >= initial_bank[idx]);
    }

    state.undo(&delta);
    assert_eq!(state.player_resources[0], initial_player);
    assert_eq!(state.bank_resources, initial_bank);
}
