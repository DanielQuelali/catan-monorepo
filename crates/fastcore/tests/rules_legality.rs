use fastcore::board::Board;
use fastcore::delta::Delta;
use fastcore::rules;
use fastcore::state::State;
use fastcore::types::{BuildingLevel, PlayerId, PortType, Resource};

fn seed_player_resources(state: &mut State, player: PlayerId, amount: u8) {
    for resource in Resource::ALL {
        state.player_resources[player as usize][resource.as_index()] = amount;
    }
}

#[test]
fn build_road_legality_requires_connection_and_resources() {
    let board = Board::standard();
    let mut state = State::new();
    let mut delta = Delta::default();

    seed_player_resources(&mut state, 0, 10);
    state.set_building(0, 0, BuildingLevel::Settlement, &mut delta);
    state.road_components[0].push_singleton(0);

    assert!(rules::is_legal_build_road(board, &state, 0, 0));

    let mut empty_state = State::new();
    seed_player_resources(&mut empty_state, 0, 10);
    assert!(!rules::is_legal_build_road(board, &empty_state, 0, 0));
}

#[test]
fn build_settlement_legality_enforces_distance_rule() {
    let board = Board::standard();
    let mut state = State::new();
    let mut delta = Delta::default();

    seed_player_resources(&mut state, 0, 10);
    state.set_building(0, 0, BuildingLevel::Settlement, &mut delta);
    state.set_road_owner(1, 0, &mut delta);
    state.set_road_owner(9, 0, &mut delta);

    assert!(rules::is_legal_build_settlement(board, &state, 0, 4));

    state.set_building(5, 1, BuildingLevel::Settlement, &mut delta);
    assert!(!rules::is_legal_build_settlement(board, &state, 0, 4));
}

#[test]
fn build_city_legality_requires_existing_settlement() {
    let board = Board::standard();
    let mut state = State::new();
    let mut delta = Delta::default();

    seed_player_resources(&mut state, 0, 10);
    assert!(!rules::is_legal_build_city(board, &state, 0, 0));

    state.set_building(0, 0, BuildingLevel::Settlement, &mut delta);
    assert!(rules::is_legal_build_city(board, &state, 0, 0));
}

#[test]
fn robber_move_legality_rejects_same_tile() {
    let board = Board::standard();
    let state = State::new();

    assert!(!rules::is_legal_move_robber(
        board,
        &state,
        state.robber_tile
    ));
    assert!(rules::is_legal_move_robber(board, &state, 1));
}

#[test]
fn maritime_trade_legality_respects_ports() {
    let board = Board::standard();
    let mut state = State::new();
    seed_player_resources(&mut state, 0, 4);
    seed_player_resources(&mut state, 1, 4);

    let brick_port = board
        .node_ports
        .iter()
        .position(|port| *port == PortType::Brick)
        .expect("brick port node");

    let three_to_one_port = board
        .node_ports
        .iter()
        .position(|port| *port == PortType::ThreeToOne)
        .expect("3:1 port node");

    let mut delta = Delta::default();
    state.set_building(brick_port as u8, 0, BuildingLevel::Settlement, &mut delta);
    state.set_building(
        three_to_one_port as u8,
        1,
        BuildingLevel::Settlement,
        &mut delta,
    );

    assert!(rules::is_legal_maritime_trade(
        board,
        &state,
        0,
        Resource::Brick,
        Resource::Grain
    ));
    state.player_resources[0][Resource::Lumber.as_index()] = 3;
    assert!(!rules::is_legal_maritime_trade(
        board,
        &state,
        0,
        Resource::Lumber,
        Resource::Grain
    ));

    assert!(rules::is_legal_maritime_trade(
        board,
        &state,
        1,
        Resource::Wool,
        Resource::Ore
    ));
}
