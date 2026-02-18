use fastcore::board::STANDARD_BOARD;
use fastcore::engine::{ArmyState, RoadState};
use fastcore::state::State;
use fastcore::types::{ActionPrompt, BuildingLevel, DevCard, INVALID_TILE, NODE_COUNT, TILE_COUNT};
use fastcore::value_player::{FastValueFunctionPlayer, ValueActionKind};
use rand_core::SeedableRng;
use rand_pcg::Pcg64Mcg;

#[test]
fn decide_prioritizes_road_building() {
    let board = STANDARD_BOARD;
    let mut state = State::new();
    state.is_initial_build_phase = false;
    state.current_prompt = ActionPrompt::PlayTurn;
    state.active_player = 0;
    state.turn_player = 0;

    let start_node = 0;
    state.node_owner[start_node] = 0;
    state.node_level[start_node] = BuildingLevel::Settlement;
    state.road_components[0].push(vec![start_node as u8]);

    let dev_idx = DevCard::RoadBuilding.as_index();
    state.dev_cards_in_hand[0][dev_idx] = 1;
    state.dev_owned_at_start[0][dev_idx] = true;

    let player = FastValueFunctionPlayer::new(None, None);
    let road_state = RoadState::empty();
    let army_state = ArmyState::empty();
    let mut rng = Pcg64Mcg::seed_from_u64(1);

    let action = player.decide(&board, &state, &road_state, &army_state, &mut rng);

    assert!(matches!(action.kind, ValueActionKind::PlayRoadBuilding));
}

#[test]
fn value_drops_when_robber_blocks_production() {
    let board = STANDARD_BOARD;
    let mut state = State::new();
    state.is_initial_build_phase = false;

    let mut selected = None;
    for node in 0..NODE_COUNT {
        for tile in board.node_tiles[node] {
            if tile == INVALID_TILE {
                continue;
            }
            let tile_idx = tile as usize;
            let number = board.tile_numbers[tile_idx];
            if board.tile_resources[tile_idx].is_some() && number.is_some() && number.unwrap() >= 2
            {
                selected = Some((node as u8, tile));
                break;
            }
        }
        if selected.is_some() {
            break;
        }
    }
    let (node, tile) = selected.expect("expected a resource tile for robber test");

    state.node_owner[node as usize] = 0;
    state.node_level[node as usize] = BuildingLevel::Settlement;

    let mut safe_tile = None;
    for tile_id in 0..TILE_COUNT {
        let tile_id = tile_id as u8;
        if tile_id == INVALID_TILE {
            continue;
        }
        if board.node_tiles[node as usize].contains(&tile_id) {
            continue;
        }
        safe_tile = Some(tile_id);
        break;
    }
    let safe_tile = safe_tile.expect("expected a non-adjacent tile for robber baseline");
    state.robber_tile = safe_tile;

    let road_state = RoadState::empty();
    let army_state = ArmyState::empty();
    let player = FastValueFunctionPlayer::new(None, None);

    let mut with_robber = state.clone();
    with_robber.robber_tile = tile;
    let value_with_robber = player.value(&board, &with_robber, &road_state, &army_state, 0);

    let value_without_robber = player.value(&board, &state, &road_state, &army_state, 0);

    assert!(
        value_with_robber < value_without_robber,
        "value_with_robber={value_with_robber} value_without_robber={value_without_robber} node={node} tile={tile}"
    );
}
