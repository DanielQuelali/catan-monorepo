use fastcore::board::STANDARD_BOARD;
use fastcore::engine::{ArmyState, RoadState};
use fastcore::rng::rng_for_stream;
use fastcore::state::State;
use fastcore::types::{
    ActionPrompt, BuildingLevel, PlayerId, Resource, PLAYER_COUNT, RESOURCE_COUNT,
};
use fastcore::value_player::{
    apply_value_action, apply_value_action_kernel, generate_playable_actions,
    FastValueFunctionPlayer, ValueAction, ValueActionKind,
};

fn assert_road_and_army_equal(
    old_road: &RoadState,
    old_army: &ArmyState,
    kernel_road: &RoadState,
    kernel_army: &ArmyState,
) {
    for player in 0..PLAYER_COUNT {
        assert_eq!(
            old_road.length_for_player(player as PlayerId),
            kernel_road.length_for_player(player as PlayerId)
        );
    }
    assert_eq!(old_road.owner(), kernel_road.owner());
    assert_eq!(old_road.length(), kernel_road.length());
    assert_eq!(old_army.owner(), kernel_army.owner());
    assert_eq!(old_army.size(), kernel_army.size());
}

fn apply_both_and_assert_equal(
    state: State,
    road_state: RoadState,
    army_state: ArmyState,
    action: ValueAction,
    seed: u64,
) {
    let board = STANDARD_BOARD;
    let mut old_state = state.clone();
    let mut old_road = road_state;
    let mut old_army = army_state;
    let mut kernel_state = state;
    let mut kernel_road = road_state;
    let mut kernel_army = army_state;
    let mut old_rng = rng_for_stream(seed, 0);
    let mut kernel_rng = rng_for_stream(seed, 0);

    apply_value_action(
        &board,
        &mut old_state,
        &mut old_road,
        &mut old_army,
        &action,
        &mut old_rng,
    );
    apply_value_action_kernel(
        &board,
        &mut kernel_state,
        &mut kernel_road,
        &mut kernel_army,
        &action,
        &mut kernel_rng,
    );

    assert_eq!(old_state, kernel_state);
    assert_road_and_army_equal(&old_road, &old_army, &kernel_road, &kernel_army);
}

fn pick_action(
    actions: &[ValueAction],
    predicate: impl Fn(&ValueActionKind) -> bool,
) -> ValueAction {
    actions
        .iter()
        .find(|action| predicate(&action.kind))
        .cloned()
        .expect("expected action for prompt scenario")
}

fn build_initial_road_scenario() -> (State, RoadState, ArmyState, ValueAction) {
    let board = STANDARD_BOARD;
    let mut state = State::new();
    let mut road_state = RoadState::empty();
    let mut army_state = ArmyState::empty();
    let settlement = generate_playable_actions(&board, &state, state.active_player)
        .into_iter()
        .next()
        .expect("expected initial settlement action");
    apply_value_action(
        &board,
        &mut state,
        &mut road_state,
        &mut army_state,
        &settlement,
        &mut rng_for_stream(7, 0),
    );
    let actions = generate_playable_actions(&board, &state, state.active_player);
    let action = pick_action(&actions, |kind| {
        matches!(kind, ValueActionKind::BuildRoad(_))
    });
    (state, road_state, army_state, action)
}

fn move_robber_scenario() -> (State, RoadState, ArmyState, ValueAction) {
    let board = STANDARD_BOARD;
    let mut state = State::new();
    state.is_initial_build_phase = false;
    state.current_prompt = ActionPrompt::MoveRobber;
    state.turn_player = 0;
    state.active_player = 0;
    state.is_moving_robber = true;

    let mut target_tile = None;
    let mut target_node = None;
    for tile in 0..board.tile_nodes.len() {
        let tile_id = tile as u8;
        if tile_id == state.robber_tile {
            continue;
        }
        for node in board.tile_nodes[tile] {
            if node == fastcore::types::INVALID_NODE {
                continue;
            }
            target_tile = Some(tile_id);
            target_node = Some(node);
            break;
        }
        if target_tile.is_some() {
            break;
        }
    }
    let tile = target_tile.expect("expected tile for robber scenario");
    let node = target_node.expect("expected node for robber scenario");
    state.node_owner[node as usize] = 1;
    state.node_level[node as usize] = BuildingLevel::Settlement;
    state.player_resources[1][Resource::Brick.as_index()] = 2;

    let actions = generate_playable_actions(&board, &state, 0);
    let action = pick_action(&actions, |kind| {
        matches!(
            kind,
            ValueActionKind::MoveRobber {
                tile: action_tile,
                victim: Some(1),
                ..
            } if *action_tile == tile
        )
    });
    (state, RoadState::empty(), ArmyState::empty(), action)
}

#[test]
fn apply_value_action_kernel_matches_prompt_classes() {
    let board = STANDARD_BOARD;

    let state = State::new();
    let actions = generate_playable_actions(&board, &state, state.active_player);
    let action = pick_action(&actions, |kind| {
        matches!(kind, ValueActionKind::BuildSettlement(_))
    });
    apply_both_and_assert_equal(state, RoadState::empty(), ArmyState::empty(), action, 1);

    let (state, road_state, army_state, action) = build_initial_road_scenario();
    apply_both_and_assert_equal(state, road_state, army_state, action, 2);

    let mut state = State::new();
    state.is_initial_build_phase = false;
    state.current_prompt = ActionPrompt::Discard;
    state.turn_player = 0;
    state.active_player = 0;
    state.is_discarding = true;
    state.player_resources[0] = [2; RESOURCE_COUNT];
    let actions = generate_playable_actions(&board, &state, 0);
    let action = pick_action(&actions, |kind| matches!(kind, ValueActionKind::Discard(_)));
    apply_both_and_assert_equal(state, RoadState::empty(), ArmyState::empty(), action, 3);

    let (state, road_state, army_state, action) = move_robber_scenario();
    apply_both_and_assert_equal(state, road_state, army_state, action, 4);

    let mut state = State::new();
    state.is_initial_build_phase = false;
    state.current_prompt = ActionPrompt::DecideTrade;
    state.turn_player = 0;
    state.active_player = 1;
    state.trade_offering_player = 0;
    state.current_trade = [0; RESOURCE_COUNT * 2];
    state.current_trade[Resource::Brick.as_index()] = 1;
    state.current_trade[RESOURCE_COUNT + Resource::Grain.as_index()] = 1;
    state.is_resolving_trade = true;
    state.player_resources[1][Resource::Grain.as_index()] = 1;
    let actions = generate_playable_actions(&board, &state, 1);
    let action = pick_action(&actions, |kind| {
        matches!(kind, ValueActionKind::AcceptTrade)
    });
    apply_both_and_assert_equal(state, RoadState::empty(), ArmyState::empty(), action, 5);

    let mut state = State::new();
    state.is_initial_build_phase = false;
    state.current_prompt = ActionPrompt::DecideAcceptees;
    state.turn_player = 0;
    state.active_player = 0;
    state.trade_offering_player = 0;
    state.is_resolving_trade = true;
    state.current_trade = [0; RESOURCE_COUNT * 2];
    state.current_trade[Resource::Brick.as_index()] = 1;
    state.current_trade[RESOURCE_COUNT + Resource::Grain.as_index()] = 1;
    state.acceptees[1] = true;
    state.player_resources[0][Resource::Brick.as_index()] = 1;
    state.player_resources[1][Resource::Grain.as_index()] = 1;
    let actions = generate_playable_actions(&board, &state, 0);
    let action = pick_action(&actions, |kind| {
        matches!(kind, ValueActionKind::ConfirmTrade(1))
    });
    apply_both_and_assert_equal(state, RoadState::empty(), ArmyState::empty(), action, 6);

    let mut state = State::new();
    state.is_initial_build_phase = false;
    state.current_prompt = ActionPrompt::PlayTurn;
    state.turn_player = 0;
    state.active_player = 0;
    state.has_rolled[0] = false;
    let actions = generate_playable_actions(&board, &state, 0);
    let action = pick_action(&actions, |kind| matches!(kind, ValueActionKind::Roll));
    apply_both_and_assert_equal(state, RoadState::empty(), ArmyState::empty(), action, 7);
}

fn player_points(
    state: &State,
    road_state: &RoadState,
    army_state: &ArmyState,
) -> [u8; PLAYER_COUNT] {
    let mut points = [0u8; PLAYER_COUNT];
    for (idx, owner) in state.node_owner.iter().enumerate() {
        if *owner == fastcore::types::NO_PLAYER {
            continue;
        }
        let add = match state.node_level[idx] {
            BuildingLevel::Settlement => 1,
            BuildingLevel::City => 2,
            BuildingLevel::Empty => 0,
        };
        points[*owner as usize] += add;
    }
    for player in 0..PLAYER_COUNT {
        points[player] +=
            state.dev_cards_in_hand[player][fastcore::types::DevCard::VictoryPoint.as_index()];
    }
    if let Some(owner) = road_state.owner() {
        points[owner as usize] += 2;
    }
    if let Some(owner) = army_state.owner() {
        points[owner as usize] += 2;
    }
    points
}

fn check_winner(state: &State, road_state: &RoadState, army_state: &ArmyState) -> Option<PlayerId> {
    let points = player_points(state, road_state, army_state);
    for (player, score) in points.iter().enumerate() {
        if *score >= 10 {
            return Some(player as u8);
        }
    }
    None
}

fn select_winner(state: &State, road_state: &RoadState, army_state: &ArmyState) -> PlayerId {
    let points = player_points(state, road_state, army_state);
    let mut best_player = 0;
    let mut best_score = points[0];
    for (player, score) in points.iter().enumerate().skip(1) {
        if *score > best_score {
            best_score = *score;
            best_player = player;
        }
    }
    best_player as PlayerId
}

fn simulate(seed: u64, max_turns: u32, kernel: bool) -> (PlayerId, u32) {
    let board = STANDARD_BOARD;
    let mut rng = rng_for_stream(seed, 0);
    let mut state = State::new_with_rng_and_board(&mut rng, &board);
    let mut road_state = RoadState::empty();
    let mut army_state = ArmyState::empty();
    let players = [
        FastValueFunctionPlayer::new(None, None),
        FastValueFunctionPlayer::new(None, None),
        FastValueFunctionPlayer::new(None, None),
        FastValueFunctionPlayer::new(None, None),
    ];

    loop {
        if state.num_turns >= max_turns {
            break;
        }
        let player = state.active_player as usize;
        let action = players[player].decide(&board, &state, &road_state, &army_state, &mut rng);
        if kernel {
            apply_value_action_kernel(
                &board,
                &mut state,
                &mut road_state,
                &mut army_state,
                &action,
                &mut rng,
            );
        } else {
            apply_value_action(
                &board,
                &mut state,
                &mut road_state,
                &mut army_state,
                &action,
                &mut rng,
            );
        }
        if check_winner(&state, &road_state, &army_state).is_some() {
            break;
        }
    }

    let winner = check_winner(&state, &road_state, &army_state)
        .unwrap_or_else(|| select_winner(&state, &road_state, &army_state));
    (winner, state.num_turns)
}

#[test]
fn kernel_trajectory_matches_legacy_over_seed_batch() {
    for seed in 1..=20_u64 {
        let old = simulate(seed, 400, false);
        let kernel = simulate(seed, 400, true);
        assert_eq!(old, kernel, "seed={seed}");
    }
}

#[test]
fn kernel_winner_and_turn_totals_match_legacy() {
    let mut old_wins = [0u32; PLAYER_COUNT];
    let mut kernel_wins = [0u32; PLAYER_COUNT];
    let mut old_turns = 0u64;
    let mut kernel_turns = 0u64;

    for seed in 50..=99_u64 {
        let old = simulate(seed, 400, false);
        let kernel = simulate(seed, 400, true);
        old_wins[old.0 as usize] += 1;
        kernel_wins[kernel.0 as usize] += 1;
        old_turns += old.1 as u64;
        kernel_turns += kernel.1 as u64;
    }

    assert_eq!(old_wins, kernel_wins);
    assert_eq!(old_turns, kernel_turns);
}
