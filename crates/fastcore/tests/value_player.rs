use fastcore::board::STANDARD_BOARD;
use fastcore::engine::{ArmyState, RoadState};
use fastcore::rng::next_u64_mod;
use fastcore::state::State;
use fastcore::types::{
    ActionPrompt, BuildingLevel, DevCard, Resource, INVALID_TILE, NODE_COUNT, TILE_COUNT,
};
use fastcore::value_player::{
    apply_value_action, generate_playable_actions, FastValueFunctionPlayer, ValueAction,
    ValueActionKind, ValueWeights,
};
use rand_core::{RngCore, SeedableRng};
use rand_pcg::Pcg64Mcg;
use std::cmp::Ordering;

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

#[test]
fn decide_clone_free_matches_clone_reference_on_city_choices() {
    let board = STANDARD_BOARD;
    let state = setup_city_choice_state();
    let road_state = RoadState::empty();
    let army_state = ArmyState::empty();
    let player = FastValueFunctionPlayer::new(None, None);
    let epsilon = None;

    let mut rng_new = Pcg64Mcg::seed_from_u64(101);
    let mut rng_ref = Pcg64Mcg::seed_from_u64(101);

    let action_new = player.decide(&board, &state, &road_state, &army_state, &mut rng_new);
    let action_ref = decide_reference_clone_path(
        &player,
        epsilon,
        &board,
        &state,
        &road_state,
        &army_state,
        &mut rng_ref,
    );

    assert_eq!(action_new, action_ref);
    assert_eq!(rng_new.next_u64(), rng_ref.next_u64());
}

#[test]
fn decide_clone_free_matches_clone_reference_with_roll_candidate() {
    let board = STANDARD_BOARD;
    let state = setup_pre_roll_monopoly_state();
    let road_state = RoadState::empty();
    let army_state = ArmyState::empty();
    let player = FastValueFunctionPlayer::new(None, None);
    let epsilon = None;

    let mut rng_new = Pcg64Mcg::seed_from_u64(202);
    let mut rng_ref = Pcg64Mcg::seed_from_u64(202);

    let action_new = player.decide(&board, &state, &road_state, &army_state, &mut rng_new);
    let action_ref = decide_reference_clone_path(
        &player,
        epsilon,
        &board,
        &state,
        &road_state,
        &army_state,
        &mut rng_ref,
    );

    assert_eq!(action_new, action_ref);
    assert_eq!(rng_new.next_u64(), rng_ref.next_u64());
}

#[test]
fn decide_clone_free_matches_clone_reference_for_epsilon_random_choice() {
    let board = STANDARD_BOARD;
    let state = setup_pre_roll_monopoly_state();
    let road_state = RoadState::empty();
    let army_state = ArmyState::empty();
    let epsilon = Some(1.0);
    let player = FastValueFunctionPlayer::new(None, epsilon);

    let mut rng_new = Pcg64Mcg::seed_from_u64(303);
    let mut rng_ref = Pcg64Mcg::seed_from_u64(303);

    let action_new = player.decide(&board, &state, &road_state, &army_state, &mut rng_new);
    let action_ref = decide_reference_clone_path(
        &player,
        epsilon,
        &board,
        &state,
        &road_state,
        &army_state,
        &mut rng_ref,
    );

    assert_eq!(action_new, action_ref);
    assert_eq!(rng_new.next_u64(), rng_ref.next_u64());
}

#[test]
fn decide_keeps_deterministic_tiebreak_when_scores_are_equal() {
    let board = STANDARD_BOARD;
    let state = setup_city_choice_state();
    let road_state = RoadState::empty();
    let army_state = ArmyState::empty();
    let player = FastValueFunctionPlayer::new(Some(zero_weights()), None);

    let mut rng = Pcg64Mcg::seed_from_u64(404);
    let action = player.decide(&board, &state, &road_state, &army_state, &mut rng);

    let mut actions = generate_playable_actions(&board, &state, state.active_player);
    let has_building_action = actions.iter().any(|candidate| {
        matches!(
            candidate.kind,
            ValueActionKind::BuildSettlement(_) | ValueActionKind::BuildCity(_)
        )
    });
    if has_building_action {
        actions.retain(|candidate| {
            matches!(
                candidate.kind,
                ValueActionKind::BuildSettlement(_) | ValueActionKind::BuildCity(_)
            )
        });
    }
    actions.sort_by(|a, b| reference_action_sort_key(a).cmp(&reference_action_sort_key(b)));

    assert!(actions.len() > 1, "expected multiple tied actions");
    assert_eq!(action, actions[0]);
}

fn setup_city_choice_state() -> State {
    let mut state = State::new();
    state.is_initial_build_phase = false;
    state.current_prompt = ActionPrompt::PlayTurn;
    state.active_player = 0;
    state.turn_player = 0;
    state.has_rolled[0] = true;

    for resource in Resource::ALL {
        state.player_resources[0][resource.as_index()] = 8;
    }

    for node in [0usize, 10usize] {
        state.node_owner[node] = 0;
        state.node_level[node] = BuildingLevel::Settlement;
        state.road_components[0].push(vec![node as u8]);
    }

    state
}

fn setup_pre_roll_monopoly_state() -> State {
    let mut state = State::new();
    state.is_initial_build_phase = false;
    state.current_prompt = ActionPrompt::PlayTurn;
    state.active_player = 0;
    state.turn_player = 0;
    state.node_owner[0] = 0;
    state.node_level[0] = BuildingLevel::Settlement;
    state.road_components[0].push(vec![0]);

    let monopoly = DevCard::Monopoly.as_index();
    state.dev_cards_in_hand[0][monopoly] = 1;
    state.dev_owned_at_start[0][monopoly] = true;

    for resource in Resource::ALL {
        state.player_resources[1][resource.as_index()] = 1;
    }

    state
}

fn decide_reference_clone_path(
    player: &FastValueFunctionPlayer,
    epsilon: Option<f64>,
    board: &fastcore::board::Board,
    state: &State,
    road_state: &RoadState,
    army_state: &ArmyState,
    rng: &mut impl RngCore,
) -> ValueAction {
    assert!(
        state.current_prompt != ActionPrompt::MoveRobber,
        "reference helper does not cover MoveRobber prompt"
    );

    let active_player = state.active_player;
    let mut actions = generate_playable_actions(board, state, active_player);
    if actions.is_empty() {
        return ValueAction {
            player: active_player,
            kind: ValueActionKind::EndTurn,
        };
    }

    let has_building_action = actions.iter().any(|action| {
        matches!(
            action.kind,
            ValueActionKind::BuildSettlement(_) | ValueActionKind::BuildCity(_)
        )
    });

    if !has_building_action {
        if let Some(action) = actions
            .iter()
            .find(|action| matches!(action.kind, ValueActionKind::PlayRoadBuilding))
        {
            return action.clone();
        }
    }

    if has_building_action {
        actions.retain(|action| {
            matches!(
                action.kind,
                ValueActionKind::BuildSettlement(_) | ValueActionKind::BuildCity(_)
            )
        });
    }

    if actions.len() == 1 {
        return actions[0].clone();
    }

    if let Some(epsilon) = epsilon {
        let roll = rng.next_u64() as f64 / (u64::MAX as f64 + 1.0);
        if roll < epsilon {
            let idx = next_u64_mod(rng, actions.len() as u64) as usize;
            return actions[idx].clone();
        }
    }

    actions.sort_by(|a, b| reference_action_sort_key(a).cmp(&reference_action_sort_key(b)));

    let mut best_value = f64::NEG_INFINITY;
    let mut best_action = actions[0].clone();
    for action in actions {
        let mut state_copy = state.clone();
        let mut road_copy = *road_state;
        let mut army_copy = *army_state;
        apply_value_action(
            board,
            &mut state_copy,
            &mut road_copy,
            &mut army_copy,
            &action,
            rng,
        );
        let value = player.value(board, &state_copy, &road_copy, &army_copy, active_player);
        if value > best_value {
            best_value = value;
            best_action = action;
        }
    }

    best_action
}

fn zero_weights() -> ValueWeights {
    ValueWeights {
        public_vps: 0.0,
        production: 0.0,
        enemy_production: 0.0,
        num_tiles: 0.0,
        reachable_production_0: 0.0,
        reachable_production_1: 0.0,
        reachable_production_2: 0.0,
        reachable_production_3: 0.0,
        buildable_nodes: 0.0,
        longest_road: 0.0,
        hand_synergy: 0.0,
        hand_resources: 0.0,
        discard_penalty: 0.0,
        devs_bought: 0.0,
        devs_in_hand_penalty: 0.0,
        army_size: 0.0,
        city_trade_gap: 0.0,
        port_trade: 0.0,
        port_trade_cap: None,
    }
}

fn reference_action_sort_key(action: &ValueAction) -> ReferenceActionSortKey {
    ReferenceActionSortKey {
        kind: reference_action_kind_name(&action.kind),
        payload: reference_action_payload_key(&action.kind),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReferenceActionSortKey {
    kind: &'static str,
    payload: Vec<ReferenceSortValue>,
}

impl Ord for ReferenceActionSortKey {
    fn cmp(&self, other: &Self) -> Ordering {
        let kind_cmp = self.kind.cmp(other.kind);
        if kind_cmp != Ordering::Equal {
            return kind_cmp;
        }
        self.payload.cmp(&other.payload)
    }
}

impl PartialOrd for ReferenceActionSortKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ReferenceSortValue {
    None,
    Int(i32),
    Str(&'static str),
}

impl Ord for ReferenceSortValue {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (ReferenceSortValue::None, ReferenceSortValue::None) => Ordering::Equal,
            (ReferenceSortValue::None, _) => Ordering::Less,
            (_, ReferenceSortValue::None) => Ordering::Greater,
            (ReferenceSortValue::Int(a), ReferenceSortValue::Int(b)) => a.cmp(b),
            (ReferenceSortValue::Str(a), ReferenceSortValue::Str(b)) => a.cmp(b),
            (ReferenceSortValue::Int(_), ReferenceSortValue::Str(_)) => Ordering::Less,
            (ReferenceSortValue::Str(_), ReferenceSortValue::Int(_)) => Ordering::Greater,
        }
    }
}

impl PartialOrd for ReferenceSortValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn reference_action_kind_name(kind: &ValueActionKind) -> &'static str {
    match kind {
        ValueActionKind::BuildSettlement(_) => "BUILD_SETTLEMENT",
        ValueActionKind::BuildRoad(_) => "BUILD_ROAD",
        ValueActionKind::BuildCity(_) => "BUILD_CITY",
        ValueActionKind::Roll => "ROLL",
        ValueActionKind::EndTurn => "END_TURN",
        ValueActionKind::Discard(_) => "DISCARD",
        ValueActionKind::MoveRobber { .. } => "MOVE_ROBBER",
        ValueActionKind::PlayYearOfPlenty(_, _) => "PLAY_YEAR_OF_PLENTY",
        ValueActionKind::PlayMonopoly(_) => "PLAY_MONOPOLY",
        ValueActionKind::PlayKnight => "PLAY_KNIGHT_CARD",
        ValueActionKind::PlayRoadBuilding => "PLAY_ROAD_BUILDING",
        ValueActionKind::MaritimeTrade { .. } => "MARITIME_TRADE",
        ValueActionKind::BuyDevCard => "BUY_DEVELOPMENT_CARD",
        ValueActionKind::AcceptTrade => "ACCEPT_TRADE",
        ValueActionKind::RejectTrade => "REJECT_TRADE",
        ValueActionKind::ConfirmTrade(_) => "CONFIRM_TRADE",
        ValueActionKind::CancelTrade => "CANCEL_TRADE",
    }
}

fn reference_action_payload_key(kind: &ValueActionKind) -> Vec<ReferenceSortValue> {
    match kind {
        ValueActionKind::BuildSettlement(node) | ValueActionKind::BuildCity(node) => {
            vec![ReferenceSortValue::Int(*node as i32)]
        }
        ValueActionKind::Discard(counts) => counts
            .as_ref()
            .map(|counts| {
                counts
                    .iter()
                    .map(|count| ReferenceSortValue::Int(*count as i32))
                    .collect()
            })
            .unwrap_or_default(),
        ValueActionKind::MoveRobber {
            tile,
            victim,
            resource,
        } => vec![
            ReferenceSortValue::Int(*tile as i32),
            victim
                .map(|id| ReferenceSortValue::Str(reference_color_name(id)))
                .unwrap_or(ReferenceSortValue::None),
            resource
                .map(|res| ReferenceSortValue::Str(reference_resource_name(res)))
                .unwrap_or(ReferenceSortValue::None),
        ],
        ValueActionKind::PlayYearOfPlenty(first, second) => {
            if let Some(second) = second {
                vec![
                    ReferenceSortValue::Str(reference_resource_name(*first)),
                    ReferenceSortValue::Str(reference_resource_name(*second)),
                ]
            } else {
                vec![ReferenceSortValue::Str(reference_resource_name(*first))]
            }
        }
        ValueActionKind::PlayMonopoly(resource) => {
            vec![ReferenceSortValue::Str(reference_resource_name(*resource))]
        }
        ValueActionKind::MaritimeTrade { offer, rate, ask } => vec![
            ReferenceSortValue::Str(reference_resource_name(*offer)),
            ReferenceSortValue::Int(*rate as i32),
            ReferenceSortValue::Str(reference_resource_name(*ask)),
        ],
        ValueActionKind::ConfirmTrade(partner) => {
            vec![ReferenceSortValue::Str(reference_color_name(*partner))]
        }
        _ => Vec::new(),
    }
}

fn reference_resource_name(resource: Resource) -> &'static str {
    match resource {
        Resource::Brick => "BRICK",
        Resource::Lumber => "WOOD",
        Resource::Ore => "ORE",
        Resource::Grain => "WHEAT",
        Resource::Wool => "SHEEP",
    }
}

fn reference_color_name(player: u8) -> &'static str {
    match player {
        0 => "RED",
        1 => "BLUE",
        2 => "ORANGE",
        3 => "WHITE",
        _ => "UNKNOWN",
    }
}
