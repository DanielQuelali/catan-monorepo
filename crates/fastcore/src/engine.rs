use crate::board::STANDARD_BOARD;
use crate::delta::Delta;
#[cfg(not(feature = "legacy_robber"))]
use crate::rng::next_u64_mod;
use crate::rng::{rng_for_stream, roll_die, shuffle_with_rng};
use crate::rules;
use crate::state::{ComponentNodeIter, State};
use crate::stats::{EvalStats, Stats};
use crate::types::{
    ActionPrompt, BuildingLevel, DevCard, EdgeId, NodeId, PlayerId, Resource, TileId, TurnPhase,
    EDGE_COUNT, INVALID_EDGE, NODE_COUNT, PLAYER_COUNT, PYTHON_RESOURCE_ORDER, RESOURCE_COUNT,
};
use rand_core::RngCore;

#[derive(Copy, Clone, Debug)]
pub struct RoadState {
    lengths: [u8; PLAYER_COUNT],
    owner: Option<PlayerId>,
    length: u8,
}

#[derive(Copy, Clone, Debug)]
pub struct ArmyState {
    owner: Option<PlayerId>,
    size: u8,
}

impl RoadState {
    pub fn empty() -> Self {
        Self {
            lengths: [0u8; PLAYER_COUNT],
            owner: None,
            length: 0,
        }
    }

    pub fn length_for_player(&self, player: PlayerId) -> u8 {
        self.lengths[player as usize]
    }

    pub fn owner(&self) -> Option<PlayerId> {
        self.owner
    }

    pub fn length(&self) -> u8 {
        self.length
    }
}

impl ArmyState {
    pub fn empty() -> Self {
        Self {
            owner: None,
            size: 0,
        }
    }

    pub fn owner(&self) -> Option<PlayerId> {
        self.owner
    }

    pub fn size(&self) -> u8 {
        self.size
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum StepResult {
    EndTurn,
    Other,
    Illegal,
}

#[derive(Copy, Clone, Debug)]
pub struct SimConfig {
    pub max_turns: u16,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self { max_turns: 2000 }
    }
}

pub fn simulate_many(seeds: &[u64], config: &SimConfig) -> Stats {
    let mut stats = Stats::default();
    for seed in seeds {
        let (game_stats, _) = simulate_one_with_log(*seed, config, 0, false);
        stats.merge(&game_stats);
    }
    stats
}

pub fn simulate_policy_log(seeds: &[u64], config: &SimConfig) -> Vec<Vec<String>> {
    let mut logs = Vec::with_capacity(seeds.len());
    for seed in seeds {
        let (_, log) = simulate_one_with_log(*seed, config, 0, true);
        logs.push(log);
    }
    logs
}

pub fn evaluate_many(seeds: &[u64], _config: &SimConfig) -> EvalStats {
    let mut stats = EvalStats::default();
    stats.games = seeds.len() as u64;
    stats
}

fn simulate_one_with_log(
    base_seed: u64,
    config: &SimConfig,
    worker_id: u64,
    with_log: bool,
) -> (Stats, Vec<String>) {
    let board = STANDARD_BOARD;
    let mut rng = rng_for_stream(base_seed, (worker_id << 32) | 0);
    let mut state = State::new_with_rng_and_board(&mut rng, &board);
    let mut stats = Stats::default();
    stats.games = 1;

    let mut road_state = RoadState {
        lengths: [0u8; PLAYER_COUNT],
        owner: None,
        length: 0,
    };
    let mut army_state = ArmyState {
        owner: None,
        size: 0,
    };
    let mut log = Vec::new();

    loop {
        if state.num_turns >= config.max_turns as u32 {
            break;
        }

        let outcome = policy_step(
            &board,
            &mut state,
            &mut rng,
            &mut road_state,
            &mut army_state,
            if with_log { Some(&mut log) } else { None },
        );

        match outcome {
            StepResult::EndTurn => {
                stats.turns += 1;
            }
            StepResult::Illegal => {
                stats.illegal_actions += 1;
                break;
            }
            StepResult::Other => {}
        }

        if check_winner(&state, &road_state, &army_state).is_some() {
            break;
        }
    }

    let winner_id = check_winner(&state, &road_state, &army_state)
        .unwrap_or_else(|| select_winner(&state, &road_state, &army_state));
    stats.wins[winner_id as usize] += 1;

    (stats, log)
}

fn policy_step(
    board: &crate::board::Board,
    state: &mut State,
    rng: &mut impl RngCore,
    road_state: &mut RoadState,
    army_state: &mut ArmyState,
    mut log: Option<&mut Vec<String>>,
) -> StepResult {
    match state.current_prompt {
        ActionPrompt::BuildInitialSettlement => {
            let node = match choose_initial_settlement(board, state) {
                Some(node) => node,
                None => return StepResult::Illegal,
            };
            apply_initial_settlement_kernel(board, state, road_state, node);
            log_action(&mut log, "BUILD_SETTLEMENT", Some(format!("{node}")));
            StepResult::Other
        }
        ActionPrompt::BuildInitialRoad => {
            let edge = match choose_initial_road(board, state) {
                Some(edge) => edge,
                None => return StepResult::Illegal,
            };
            apply_initial_road_kernel(board, state, road_state, edge);
            log_action(&mut log, "BUILD_ROAD", Some(format!("{edge}")));
            StepResult::Other
        }
        ActionPrompt::Discard => {
            let player = state.active_player;
            let discard = choose_discard(state, player, rng);
            apply_discard_kernel(state, player, &discard);
            log_action(&mut log, "DISCARD", Some(format_counts(&discard)));
            StepResult::Other
        }
        ActionPrompt::MoveRobber => {
            let player = state.active_player;
            let (tile, victim, resource) =
                choose_robber_move(board, state, road_state, army_state, player, rng);
            apply_move_robber_kernel(state, tile, victim, resource);
            log_action(
                &mut log,
                "MOVE_ROBBER",
                Some(format_robber_payload(tile, victim, resource)),
            );
            StepResult::Other
        }
        ActionPrompt::DecideTrade => {
            let player = state.active_player;
            if can_accept_trade(state, player) {
                apply_accept_trade(state, player);
                log_action(
                    &mut log,
                    "ACCEPT_TRADE",
                    Some(format_trade_payload(state.current_trade)),
                );
            } else {
                apply_reject_trade(state, player);
                log_action(
                    &mut log,
                    "REJECT_TRADE",
                    Some(format_trade_payload(state.current_trade)),
                );
            }
            StepResult::Other
        }
        ActionPrompt::DecideAcceptees => {
            if let Some(partner) = first_acceptee(state) {
                let trade = state.current_trade;
                apply_confirm_trade_kernel(state, partner);
                log_action(
                    &mut log,
                    "CONFIRM_TRADE",
                    Some(format_confirm_trade_payload(trade, partner)),
                );
            } else {
                apply_cancel_trade(state);
                log_action(&mut log, "CANCEL_TRADE", None);
            }
            StepResult::Other
        }
        ActionPrompt::PlayTurn => {
            play_turn_step(board, state, rng, road_state, army_state, &mut log)
        }
    }
}

fn play_turn_step(
    board: &crate::board::Board,
    state: &mut State,
    rng: &mut impl RngCore,
    road_state: &mut RoadState,
    army_state: &mut ArmyState,
    log: &mut Option<&mut Vec<String>>,
) -> StepResult {
    let player = state.turn_player;

    if state.is_road_building {
        if let Some(edge) = choose_free_road(board, state, player) {
            apply_build_road_kernel(board, state, road_state, player, edge, true);
            log_action(log, "BUILD_ROAD", Some(format!("{edge}")));
            state.free_roads_available = state.free_roads_available.saturating_sub(1);
            if state.free_roads_available == 0 || choose_free_road(board, state, player).is_none() {
                state.is_road_building = false;
                state.free_roads_available = 0;
            }
            return StepResult::Other;
        }
        state.is_road_building = false;
        state.free_roads_available = 0;
    }

    if let Some((card, payload)) = choose_dev_play(board, state) {
        apply_dev_play_kernel(board, state, road_state, army_state, card, payload, log);
        return StepResult::Other;
    }

    if !state.has_rolled[player as usize] {
        let roll = (roll_die(rng) as u32, roll_die(rng) as u32);
        apply_roll_kernel(board, state, roll);
        log_action(log, "ROLL", Some(format!("{},{}", roll.0, roll.1)));
        return StepResult::Other;
    }

    if let Some(node) = choose_build_city(board, state, player) {
        apply_build_city_kernel(board, state, player, node);
        log_action(log, "BUILD_CITY", Some(format!("{node}")));
        return StepResult::Other;
    }
    if let Some(node) = choose_build_settlement(board, state, player) {
        apply_build_settlement_kernel(board, state, road_state, player, node);
        log_action(log, "BUILD_SETTLEMENT", Some(format!("{node}")));
        return StepResult::Other;
    }
    if let Some(edge) = choose_build_road(board, state, player) {
        apply_build_road_kernel(board, state, road_state, player, edge, false);
        log_action(log, "BUILD_ROAD", Some(format!("{edge}")));
        return StepResult::Other;
    }
    if can_buy_dev_card(state, player) {
        let card = buy_dev_card_kernel(state, player);
        log_action(log, "BUY_DEV_CARD", Some(format_dev_card(card)));
        return StepResult::Other;
    }
    if let Some((offer, rate, ask)) = choose_maritime_trade(board, state, player) {
        apply_maritime_trade_kernel(state, player, offer, rate, ask);
        log_action(
            log,
            "MARITIME_TRADE",
            Some(format_maritime_payload(offer, rate, ask)),
        );
        return StepResult::Other;
    }
    if !state.trade_offered_this_turn {
        if let Some(trade) = choose_domestic_trade(state, player) {
            apply_offer_trade(state, player, &trade);
            log_action(log, "OFFER_TRADE", Some(format_trade_payload(trade)));
            return StepResult::Other;
        }
    }

    apply_end_turn(state, player);
    log_action(log, "END_TURN", None);
    StepResult::EndTurn
}

fn choose_initial_settlement(board: &crate::board::Board, state: &State) -> Option<NodeId> {
    for node in 0..NODE_COUNT {
        let node_id = node as NodeId;
        if rules::is_legal_initial_settlement(board, state, node_id) {
            return Some(node_id);
        }
    }
    None
}

fn choose_initial_road(board: &crate::board::Board, state: &State) -> Option<EdgeId> {
    let anchor = state.last_initial_settlement[state.active_player as usize];
    if anchor == crate::types::INVALID_NODE {
        return None;
    }
    for edge in board.node_edges[anchor as usize] {
        if edge == crate::types::INVALID_EDGE {
            continue;
        }
        if rules::is_legal_initial_road(board, state, state.active_player, edge, anchor) {
            return Some(edge);
        }
    }
    None
}

fn choose_build_city(
    board: &crate::board::Board,
    state: &State,
    player: PlayerId,
) -> Option<NodeId> {
    for node in 0..NODE_COUNT {
        let node_id = node as NodeId;
        if rules::is_legal_build_city(board, state, player, node_id) {
            return Some(node_id);
        }
    }
    None
}

fn choose_build_settlement(
    board: &crate::board::Board,
    state: &State,
    player: PlayerId,
) -> Option<NodeId> {
    for node in 0..NODE_COUNT {
        let node_id = node as NodeId;
        if rules::is_legal_build_settlement(board, state, player, node_id) {
            return Some(node_id);
        }
    }
    None
}

fn choose_build_road(
    board: &crate::board::Board,
    state: &State,
    player: PlayerId,
) -> Option<EdgeId> {
    for edge in 0..EDGE_COUNT {
        let edge_id = edge as EdgeId;
        if rules::is_legal_build_road(board, state, player, edge_id) {
            return Some(edge_id);
        }
    }
    None
}

fn choose_free_road(
    board: &crate::board::Board,
    state: &State,
    player: PlayerId,
) -> Option<EdgeId> {
    for edge in 0..EDGE_COUNT {
        let edge_id = edge as EdgeId;
        if is_legal_build_road_free(board, state, player, edge_id) {
            return Some(edge_id);
        }
    }
    None
}

fn choose_dev_play(board: &crate::board::Board, state: &State) -> Option<(DevCard, DevPayload)> {
    let player = state.turn_player;
    if !can_play_dev(state, player, DevCard::YearOfPlenty) {
        // continue
    } else if let Some(payload) = choose_year_of_plenty(state) {
        return Some((DevCard::YearOfPlenty, payload));
    }
    if can_play_dev(state, player, DevCard::Monopoly) {
        if let Some(resource) = choose_monopoly(state, player) {
            return Some((DevCard::Monopoly, DevPayload::Monopoly(resource)));
        }
    }
    if can_play_dev(state, player, DevCard::Knight) {
        return Some((DevCard::Knight, DevPayload::None));
    }
    if can_play_dev(state, player, DevCard::RoadBuilding) {
        if choose_free_road(board, state, player).is_some() {
            return Some((DevCard::RoadBuilding, DevPayload::None));
        }
    }
    None
}

#[derive(Copy, Clone, Debug)]
enum DevPayload {
    None,
    YearOfPlenty(Resource, Option<Resource>),
    Monopoly(Resource),
}

fn choose_year_of_plenty(state: &State) -> Option<DevPayload> {
    let mut first: Option<Resource> = None;
    let mut second: Option<Resource> = None;
    for resource in Resource::ALL {
        if state.bank_resources[resource.as_index()] > 0 {
            first = Some(resource);
            break;
        }
    }
    let first = first?;
    if state.bank_resources[first.as_index()] > 1 {
        second = Some(first);
    } else {
        for resource in Resource::ALL {
            if resource == first {
                continue;
            }
            if state.bank_resources[resource.as_index()] > 0 {
                second = Some(resource);
                break;
            }
        }
    }
    Some(DevPayload::YearOfPlenty(first, second))
}

fn choose_monopoly(state: &State, player: PlayerId) -> Option<Resource> {
    for resource in Resource::ALL {
        let mut total = 0u32;
        for other in 0..PLAYER_COUNT {
            if other as u8 == player {
                continue;
            }
            total += state.player_resources[other][resource.as_index()] as u32;
        }
        if total > 0 {
            return Some(resource);
        }
    }
    None
}

pub(crate) fn can_buy_dev_card(state: &State, player: PlayerId) -> bool {
    if state.dev_deck.is_empty() {
        return false;
    }
    let hand = &state.player_resources[player as usize];
    hand[Resource::Wool.as_index()] >= 1
        && hand[Resource::Grain.as_index()] >= 1
        && hand[Resource::Ore.as_index()] >= 1
}

pub(crate) fn buy_dev_card(state: &mut State, player: PlayerId) -> DevCard {
    let mut delta = Delta::default();
    pay_cost(state, player, rules::dev_card_cost(), &mut delta);
    let card = state.dev_deck.pop().unwrap_or(DevCard::Knight);
    let idx = card.as_index();
    state.dev_cards_in_hand[player as usize][idx] += 1;
    card
}

pub(crate) fn buy_dev_card_kernel(state: &mut State, player: PlayerId) -> DevCard {
    pay_cost_kernel(state, player, rules::dev_card_cost());
    let card = state.dev_deck.pop().unwrap_or(DevCard::Knight);
    let idx = card.as_index();
    state.dev_cards_in_hand[player as usize][idx] += 1;
    card
}

fn choose_maritime_trade(
    board: &crate::board::Board,
    state: &State,
    player: PlayerId,
) -> Option<(Resource, u8, Resource)> {
    for offer in Resource::ALL {
        let rate = trade_rate(board, state, player, offer);
        if state.player_resources[player as usize][offer.as_index()] < rate {
            continue;
        }
        for ask in Resource::ALL {
            if ask == offer {
                continue;
            }
            if state.bank_resources[ask.as_index()] > 0 {
                return Some((offer, rate, ask));
            }
        }
    }
    None
}

fn choose_domestic_trade(state: &State, player: PlayerId) -> Option<[u8; RESOURCE_COUNT * 2]> {
    let mut offer = [0u8; RESOURCE_COUNT];
    let mut ask = [0u8; RESOURCE_COUNT];

    let mut offer_resource: Option<Resource> = None;
    for resource in Resource::ALL {
        if state.player_resources[player as usize][resource.as_index()] > 0 {
            offer_resource = Some(resource);
            break;
        }
    }
    let offer_resource = offer_resource?;

    let mut ask_resource: Option<Resource> = None;
    for other in 0..PLAYER_COUNT {
        if other as u8 == player {
            continue;
        }
        for resource in Resource::ALL {
            if resource == offer_resource {
                continue;
            }
            if state.player_resources[other][resource.as_index()] > 0 {
                ask_resource = Some(resource);
                break;
            }
        }
        if ask_resource.is_some() {
            break;
        }
    }
    let ask_resource = ask_resource?;

    offer[offer_resource.as_index()] = 1;
    ask[ask_resource.as_index()] = 1;

    let mut trade = [0u8; RESOURCE_COUNT * 2];
    trade[..RESOURCE_COUNT].copy_from_slice(&offer);
    trade[RESOURCE_COUNT..].copy_from_slice(&ask);
    Some(trade)
}

pub(crate) fn apply_initial_settlement(
    board: &crate::board::Board,
    state: &mut State,
    _road_state: &mut RoadState,
    node: NodeId,
) {
    let player = state.active_player;
    let mut delta = Delta::default();
    state.set_building(node, player, BuildingLevel::Settlement, &mut delta);
    state.last_initial_settlement[player as usize] = node;
    state.road_components[player as usize].push_singleton(node);

    let count = player_settlement_count(state, player);
    if count == 2 {
        for tile in board.node_tiles[node as usize] {
            if tile == crate::types::INVALID_TILE {
                continue;
            }
            if let Some(resource) = board.tile_resources[tile as usize] {
                if state.bank_resources[resource.as_index()] == 0 {
                    continue;
                }
                state.adjust_resource(player, resource, 1, &mut delta);
                state.adjust_bank(resource, -1, &mut delta);
            }
        }
    }

    state.current_prompt = ActionPrompt::BuildInitialRoad;
}

pub(crate) fn apply_initial_settlement_kernel(
    board: &crate::board::Board,
    state: &mut State,
    _road_state: &mut RoadState,
    node: NodeId,
) {
    let player = state.active_player;
    state.set_building_kernel(node, player, BuildingLevel::Settlement);
    state.last_initial_settlement[player as usize] = node;
    state.road_components[player as usize].push_singleton(node);

    let count = player_settlement_count(state, player);
    if count == 2 {
        for tile in board.node_tiles[node as usize] {
            if tile == crate::types::INVALID_TILE {
                continue;
            }
            if let Some(resource) = board.tile_resources[tile as usize] {
                if state.bank_resources[resource.as_index()] == 0 {
                    continue;
                }
                state.adjust_resource_kernel(player, resource, 1);
                state.adjust_bank_kernel(resource, -1);
            }
        }
    }

    state.current_prompt = ActionPrompt::BuildInitialRoad;
}

pub(crate) fn apply_initial_road(
    board: &crate::board::Board,
    state: &mut State,
    road_state: &mut RoadState,
    edge: EdgeId,
) {
    let player = state.active_player;
    let mut delta = Delta::default();
    state.set_road_owner(edge, player, &mut delta);
    update_components_on_build_road(board, state, player, edge);
    update_longest_road_after_build_road(board, state, road_state, player);

    let num_buildings = total_settlement_count(state);
    let num_players = PLAYER_COUNT as u8;
    let going_forward = num_buildings < num_players;
    let at_the_end = num_buildings == num_players;
    if going_forward {
        advance_turn(state, 1);
        state.current_prompt = ActionPrompt::BuildInitialSettlement;
    } else if at_the_end {
        state.current_prompt = ActionPrompt::BuildInitialSettlement;
    } else if num_buildings == 2 * num_players {
        state.is_initial_build_phase = false;
        state.current_prompt = ActionPrompt::PlayTurn;
        state.turn_phase = TurnPhase::Roll;
    } else {
        advance_turn(state, -1);
        state.current_prompt = ActionPrompt::BuildInitialSettlement;
    }
}

pub(crate) fn apply_initial_road_kernel(
    board: &crate::board::Board,
    state: &mut State,
    road_state: &mut RoadState,
    edge: EdgeId,
) {
    let player = state.active_player;
    state.set_road_owner_kernel(edge, player);
    update_components_on_build_road(board, state, player, edge);
    update_longest_road_after_build_road(board, state, road_state, player);

    let num_buildings = total_settlement_count(state);
    let num_players = PLAYER_COUNT as u8;
    let going_forward = num_buildings < num_players;
    let at_the_end = num_buildings == num_players;
    if going_forward {
        advance_turn(state, 1);
        state.current_prompt = ActionPrompt::BuildInitialSettlement;
    } else if at_the_end {
        state.current_prompt = ActionPrompt::BuildInitialSettlement;
    } else if num_buildings == 2 * num_players {
        state.is_initial_build_phase = false;
        state.current_prompt = ActionPrompt::PlayTurn;
        state.turn_phase = TurnPhase::Roll;
    } else {
        advance_turn(state, -1);
        state.current_prompt = ActionPrompt::BuildInitialSettlement;
    }
}

pub(crate) fn apply_build_city(
    board: &crate::board::Board,
    state: &mut State,
    player: PlayerId,
    node: NodeId,
) {
    let mut delta = Delta::default();
    pay_cost(state, player, rules::city_cost(), &mut delta);
    state.set_building(node, player, BuildingLevel::City, &mut delta);
    let _ = board;
}

pub(crate) fn apply_build_city_kernel(
    board: &crate::board::Board,
    state: &mut State,
    player: PlayerId,
    node: NodeId,
) {
    pay_cost_kernel(state, player, rules::city_cost());
    state.set_building_kernel(node, player, BuildingLevel::City);
    let _ = board;
}

pub(crate) fn apply_build_settlement(
    board: &crate::board::Board,
    state: &mut State,
    road_state: &mut RoadState,
    player: PlayerId,
    node: NodeId,
) {
    let mut delta = Delta::default();
    pay_cost(state, player, rules::settlement_cost(), &mut delta);
    state.set_building(node, player, BuildingLevel::Settlement, &mut delta);
    let previous_owner = road_state.owner;
    let plowed = update_components_on_build_settlement(board, state, player, node);
    if !plowed.is_empty() {
        update_longest_road_after_plow(board, state, road_state, &plowed, previous_owner);
    }
}

pub(crate) fn apply_build_settlement_kernel(
    board: &crate::board::Board,
    state: &mut State,
    road_state: &mut RoadState,
    player: PlayerId,
    node: NodeId,
) {
    pay_cost_kernel(state, player, rules::settlement_cost());
    state.set_building_kernel(node, player, BuildingLevel::Settlement);
    let previous_owner = road_state.owner;
    let plowed = update_components_on_build_settlement(board, state, player, node);
    if !plowed.is_empty() {
        update_longest_road_after_plow(board, state, road_state, &plowed, previous_owner);
    }
}

pub(crate) fn apply_build_road(
    board: &crate::board::Board,
    state: &mut State,
    road_state: &mut RoadState,
    player: PlayerId,
    edge: EdgeId,
    free: bool,
) {
    let mut delta = Delta::default();
    if !free {
        pay_cost(state, player, rules::road_cost(), &mut delta);
    }
    state.set_road_owner(edge, player, &mut delta);
    update_components_on_build_road(board, state, player, edge);
    update_longest_road_after_build_road(board, state, road_state, player);
}

pub(crate) fn apply_build_road_kernel(
    board: &crate::board::Board,
    state: &mut State,
    road_state: &mut RoadState,
    player: PlayerId,
    edge: EdgeId,
    free: bool,
) {
    if !free {
        pay_cost_kernel(state, player, rules::road_cost());
    }
    state.set_road_owner_kernel(edge, player);
    update_components_on_build_road(board, state, player, edge);
    update_longest_road_after_build_road(board, state, road_state, player);
}

pub(crate) fn apply_roll(board: &crate::board::Board, state: &mut State, roll: (u32, u32)) {
    let player = state.turn_player;
    state.has_rolled[player as usize] = true;
    let total = (roll.0 + roll.1) as u8;

    if total == 7 {
        let mut any_discard = false;
        for idx in 0..PLAYER_COUNT {
            if player_resource_total(state, idx as u8) > 7 {
                any_discard = true;
                break;
            }
        }
        if any_discard {
            state.active_player = first_discarder(state);
            state.current_prompt = ActionPrompt::Discard;
            state.is_discarding = true;
        } else {
            state.current_prompt = ActionPrompt::MoveRobber;
            state.is_moving_robber = true;
            state.active_player = player;
        }
    } else {
        distribute_resources(board, state, total);
        state.current_prompt = ActionPrompt::PlayTurn;
        state.turn_phase = TurnPhase::Main;
    }
}

pub(crate) fn apply_roll_kernel(board: &crate::board::Board, state: &mut State, roll: (u32, u32)) {
    let player = state.turn_player;
    state.has_rolled[player as usize] = true;
    let total = (roll.0 + roll.1) as u8;

    if total == 7 {
        let mut any_discard = false;
        for idx in 0..PLAYER_COUNT {
            if player_resource_total(state, idx as u8) > 7 {
                any_discard = true;
                break;
            }
        }
        if any_discard {
            state.active_player = first_discarder(state);
            state.current_prompt = ActionPrompt::Discard;
            state.is_discarding = true;
        } else {
            state.current_prompt = ActionPrompt::MoveRobber;
            state.is_moving_robber = true;
            state.active_player = player;
        }
    } else {
        distribute_resources_kernel(board, state, total);
        state.current_prompt = ActionPrompt::PlayTurn;
        state.turn_phase = TurnPhase::Main;
    }
}

pub(crate) fn apply_discard(state: &mut State, player: PlayerId, counts: &[u8; RESOURCE_COUNT]) {
    let mut delta = Delta::default();
    for resource in Resource::ALL {
        let idx = resource.as_index();
        let amount = counts[idx];
        if amount == 0 {
            continue;
        }
        state.adjust_resource(player, resource, -(amount as i8), &mut delta);
        state.adjust_bank(resource, amount as i8, &mut delta);
    }

    if let Some(next) = next_discarder_after(state, state.active_player) {
        state.active_player = next;
    } else {
        state.active_player = state.turn_player;
        state.current_prompt = ActionPrompt::MoveRobber;
        state.is_discarding = false;
        state.is_moving_robber = true;
    }
}

pub(crate) fn apply_discard_kernel(
    state: &mut State,
    player: PlayerId,
    counts: &[u8; RESOURCE_COUNT],
) {
    for resource in Resource::ALL {
        let idx = resource.as_index();
        let amount = counts[idx];
        if amount == 0 {
            continue;
        }
        state.adjust_resource_kernel(player, resource, -(amount as i8));
        state.adjust_bank_kernel(resource, amount as i8);
    }

    if let Some(next) = next_discarder_after(state, state.active_player) {
        state.active_player = next;
    } else {
        state.active_player = state.turn_player;
        state.current_prompt = ActionPrompt::MoveRobber;
        state.is_discarding = false;
        state.is_moving_robber = true;
    }
}

pub(crate) fn apply_move_robber(
    state: &mut State,
    tile: TileId,
    victim: Option<PlayerId>,
    resource: Option<Resource>,
) {
    let mut delta = Delta::default();
    state.move_robber(tile, &mut delta);
    if let (Some(victim), Some(resource)) = (victim, resource) {
        if state.player_resources[victim as usize][resource.as_index()] > 0 {
            state.adjust_resource(victim, resource, -1, &mut delta);
            state.adjust_resource(state.turn_player, resource, 1, &mut delta);
        }
    }
    state.current_prompt = ActionPrompt::PlayTurn;
    state.is_moving_robber = false;
    state.turn_phase = TurnPhase::Main;
}

pub(crate) fn apply_move_robber_kernel(
    state: &mut State,
    tile: TileId,
    victim: Option<PlayerId>,
    resource: Option<Resource>,
) {
    state.move_robber_kernel(tile);
    if let (Some(victim), Some(resource)) = (victim, resource) {
        if state.player_resources[victim as usize][resource.as_index()] > 0 {
            state.adjust_resource_kernel(victim, resource, -1);
            state.adjust_resource_kernel(state.turn_player, resource, 1);
        }
    }
    state.current_prompt = ActionPrompt::PlayTurn;
    state.is_moving_robber = false;
    state.turn_phase = TurnPhase::Main;
}

pub(crate) fn apply_end_turn(state: &mut State, player: PlayerId) {
    clean_turn(state, player);
    advance_turn(state, 1);
    state.current_prompt = ActionPrompt::PlayTurn;
    state.turn_phase = TurnPhase::Roll;
}

#[allow(dead_code)]
fn apply_dev_play(
    board: &crate::board::Board,
    state: &mut State,
    road_state: &mut RoadState,
    army_state: &mut ArmyState,
    card: DevCard,
    payload: DevPayload,
    log: &mut Option<&mut Vec<String>>,
) {
    match card {
        DevCard::YearOfPlenty => {
            if let DevPayload::YearOfPlenty(first, second) = payload {
                apply_year_of_plenty(state, first, second);
                log_action(
                    log,
                    "PLAY_YEAR_OF_PLENTY",
                    Some(format_year_of_plenty_payload(first, second)),
                );
            }
        }
        DevCard::Monopoly => {
            if let DevPayload::Monopoly(resource) = payload {
                apply_monopoly(state, resource);
                log_action(log, "PLAY_MONOPOLY", Some(format_resource(resource)));
            }
        }
        DevCard::Knight => {
            apply_knight(state, army_state);
            log_action(log, "PLAY_KNIGHT", None);
            state.current_prompt = ActionPrompt::MoveRobber;
            state.is_moving_robber = true;
        }
        DevCard::RoadBuilding => {
            apply_road_building(state);
            log_action(log, "PLAY_ROAD_BUILDING", None);
        }
        DevCard::VictoryPoint => {}
    }
    let _ = board;
    let _ = road_state;
}

fn apply_dev_play_kernel(
    board: &crate::board::Board,
    state: &mut State,
    road_state: &mut RoadState,
    army_state: &mut ArmyState,
    card: DevCard,
    payload: DevPayload,
    log: &mut Option<&mut Vec<String>>,
) {
    match card {
        DevCard::YearOfPlenty => {
            if let DevPayload::YearOfPlenty(first, second) = payload {
                apply_year_of_plenty_kernel(state, first, second);
                log_action(
                    log,
                    "PLAY_YEAR_OF_PLENTY",
                    Some(format_year_of_plenty_payload(first, second)),
                );
            }
        }
        DevCard::Monopoly => {
            if let DevPayload::Monopoly(resource) = payload {
                apply_monopoly_kernel(state, resource);
                log_action(log, "PLAY_MONOPOLY", Some(format_resource(resource)));
            }
        }
        DevCard::Knight => {
            apply_knight(state, army_state);
            log_action(log, "PLAY_KNIGHT", None);
            state.current_prompt = ActionPrompt::MoveRobber;
            state.is_moving_robber = true;
        }
        DevCard::RoadBuilding => {
            apply_road_building(state);
            log_action(log, "PLAY_ROAD_BUILDING", None);
        }
        DevCard::VictoryPoint => {}
    }
    let _ = board;
    let _ = road_state;
}

pub(crate) fn apply_year_of_plenty(state: &mut State, first: Resource, second: Option<Resource>) {
    let mut delta = Delta::default();
    if state.bank_resources[first.as_index()] > 0 {
        state.adjust_resource(state.turn_player, first, 1, &mut delta);
        state.adjust_bank(first, -1, &mut delta);
    }
    if let Some(second) = second {
        if state.bank_resources[second.as_index()] > 0 {
            state.adjust_resource(state.turn_player, second, 1, &mut delta);
            state.adjust_bank(second, -1, &mut delta);
        }
    }
    mark_dev_played(state, state.turn_player, DevCard::YearOfPlenty);
}

pub(crate) fn apply_year_of_plenty_kernel(
    state: &mut State,
    first: Resource,
    second: Option<Resource>,
) {
    if state.bank_resources[first.as_index()] > 0 {
        state.adjust_resource_kernel(state.turn_player, first, 1);
        state.adjust_bank_kernel(first, -1);
    }
    if let Some(second) = second {
        if state.bank_resources[second.as_index()] > 0 {
            state.adjust_resource_kernel(state.turn_player, second, 1);
            state.adjust_bank_kernel(second, -1);
        }
    }
    mark_dev_played(state, state.turn_player, DevCard::YearOfPlenty);
}

pub(crate) fn apply_monopoly(state: &mut State, resource: Resource) {
    let mut delta = Delta::default();
    let mut total = 0u8;
    for other in 0..PLAYER_COUNT {
        if other as u8 == state.turn_player {
            continue;
        }
        let count = state.player_resources[other][resource.as_index()];
        if count > 0 {
            state.adjust_resource(other as u8, resource, -(count as i8), &mut delta);
            total += count;
        }
    }
    if total > 0 {
        state.adjust_resource(state.turn_player, resource, total as i8, &mut delta);
    }
    mark_dev_played(state, state.turn_player, DevCard::Monopoly);
}

pub(crate) fn apply_monopoly_kernel(state: &mut State, resource: Resource) {
    let mut total = 0u8;
    for other in 0..PLAYER_COUNT {
        if other as u8 == state.turn_player {
            continue;
        }
        let count = state.player_resources[other][resource.as_index()];
        if count > 0 {
            state.adjust_resource_kernel(other as u8, resource, -(count as i8));
            total += count;
        }
    }
    if total > 0 {
        state.adjust_resource_kernel(state.turn_player, resource, total as i8);
    }
    mark_dev_played(state, state.turn_player, DevCard::Monopoly);
}

pub(crate) fn apply_knight(state: &mut State, army_state: &mut ArmyState) {
    let player = state.turn_player;
    let previous_owner = army_state.owner;
    let previous_size = previous_owner
        .map(|owner| state.dev_cards_played[owner as usize][DevCard::Knight.as_index()])
        .unwrap_or(0);
    mark_dev_played(state, player, DevCard::Knight);
    update_largest_army(state, army_state, player, previous_owner, previous_size);
}

pub(crate) fn apply_road_building(state: &mut State) {
    mark_dev_played(state, state.turn_player, DevCard::RoadBuilding);
    state.is_road_building = true;
    state.free_roads_available = 2;
}

fn mark_dev_played(state: &mut State, player: PlayerId, card: DevCard) {
    let idx = card.as_index();
    if state.dev_cards_in_hand[player as usize][idx] == 0 {
        return;
    }
    state.dev_cards_in_hand[player as usize][idx] -= 1;
    state.dev_cards_played[player as usize][idx] += 1;
    state.has_played_dev[player as usize] = true;
}

pub(crate) fn apply_offer_trade(
    state: &mut State,
    player: PlayerId,
    trade: &[u8; RESOURCE_COUNT * 2],
) {
    state.is_resolving_trade = true;
    state.current_trade.copy_from_slice(trade);
    state.trade_offering_player = player;
    state.acceptees = [false; PLAYER_COUNT];
    state.trade_offered_this_turn = true;

    state.active_player = first_trade_responder(player).unwrap_or(player);
    state.current_prompt = ActionPrompt::DecideTrade;
}

pub(crate) fn apply_accept_trade(state: &mut State, player: PlayerId) {
    state.acceptees[player as usize] = true;
    if let Some(next) = next_trade_responder(state, player) {
        state.active_player = next;
    } else {
        state.active_player = state.trade_offering_player;
        state.current_prompt = ActionPrompt::DecideAcceptees;
    }
}

pub(crate) fn apply_reject_trade(state: &mut State, player: PlayerId) {
    if let Some(next) = next_trade_responder(state, player) {
        state.active_player = next;
    } else if state.acceptees.iter().any(|accepted| *accepted) {
        state.active_player = state.trade_offering_player;
        state.current_prompt = ActionPrompt::DecideAcceptees;
    } else {
        reset_trade_state(state);
        state.active_player = state.turn_player;
        state.current_prompt = ActionPrompt::PlayTurn;
    }
}

pub(crate) fn apply_confirm_trade(state: &mut State, partner: PlayerId) {
    let mut offer = [0u8; RESOURCE_COUNT];
    let mut ask = [0u8; RESOURCE_COUNT];
    offer.copy_from_slice(&state.current_trade[..RESOURCE_COUNT]);
    ask.copy_from_slice(&state.current_trade[RESOURCE_COUNT..]);
    let mut delta = Delta::default();

    for (idx, amount) in offer.iter().enumerate() {
        if *amount == 0 {
            continue;
        }
        let resource = Resource::from_index(idx).unwrap();
        state.adjust_resource(
            state.trade_offering_player,
            resource,
            -(*amount as i8),
            &mut delta,
        );
        state.adjust_resource(partner, resource, *amount as i8, &mut delta);
    }
    for (idx, amount) in ask.iter().enumerate() {
        if *amount == 0 {
            continue;
        }
        let resource = Resource::from_index(idx).unwrap();
        state.adjust_resource(partner, resource, -(*amount as i8), &mut delta);
        state.adjust_resource(
            state.trade_offering_player,
            resource,
            *amount as i8,
            &mut delta,
        );
    }

    reset_trade_state(state);
    state.active_player = state.turn_player;
    state.current_prompt = ActionPrompt::PlayTurn;
}

pub(crate) fn apply_confirm_trade_kernel(state: &mut State, partner: PlayerId) {
    let mut offer = [0u8; RESOURCE_COUNT];
    let mut ask = [0u8; RESOURCE_COUNT];
    offer.copy_from_slice(&state.current_trade[..RESOURCE_COUNT]);
    ask.copy_from_slice(&state.current_trade[RESOURCE_COUNT..]);

    for (idx, amount) in offer.iter().enumerate() {
        if *amount == 0 {
            continue;
        }
        let resource = Resource::from_index(idx).unwrap();
        state.adjust_resource_kernel(state.trade_offering_player, resource, -(*amount as i8));
        state.adjust_resource_kernel(partner, resource, *amount as i8);
    }
    for (idx, amount) in ask.iter().enumerate() {
        if *amount == 0 {
            continue;
        }
        let resource = Resource::from_index(idx).unwrap();
        state.adjust_resource_kernel(partner, resource, -(*amount as i8));
        state.adjust_resource_kernel(state.trade_offering_player, resource, *amount as i8);
    }

    reset_trade_state(state);
    state.active_player = state.turn_player;
    state.current_prompt = ActionPrompt::PlayTurn;
}

pub(crate) fn apply_cancel_trade(state: &mut State) {
    reset_trade_state(state);
    state.active_player = state.turn_player;
    state.current_prompt = ActionPrompt::PlayTurn;
}

pub(crate) fn apply_maritime_trade(
    state: &mut State,
    player: PlayerId,
    offer: Resource,
    rate: u8,
    ask: Resource,
) {
    let mut delta = Delta::default();
    state.adjust_resource(player, offer, -(rate as i8), &mut delta);
    state.adjust_bank(offer, rate as i8, &mut delta);
    state.adjust_bank(ask, -1, &mut delta);
    state.adjust_resource(player, ask, 1, &mut delta);
}

pub(crate) fn apply_maritime_trade_kernel(
    state: &mut State,
    player: PlayerId,
    offer: Resource,
    rate: u8,
    ask: Resource,
) {
    state.adjust_resource_kernel(player, offer, -(rate as i8));
    state.adjust_bank_kernel(offer, rate as i8);
    state.adjust_bank_kernel(ask, -1);
    state.adjust_resource_kernel(player, ask, 1);
}

fn reset_trade_state(state: &mut State) {
    state.is_resolving_trade = false;
    state.current_trade = [0; RESOURCE_COUNT * 2];
    state.acceptees = [false; PLAYER_COUNT];
}

pub(crate) fn can_accept_trade(state: &State, player: PlayerId) -> bool {
    let ask = &state.current_trade[RESOURCE_COUNT..];
    ask.iter()
        .enumerate()
        .all(|(idx, amount)| state.player_resources[player as usize][idx] >= *amount)
}

fn first_acceptee(state: &State) -> Option<PlayerId> {
    for (idx, accepted) in state.acceptees.iter().enumerate() {
        if *accepted {
            return Some(idx as u8);
        }
    }
    None
}

fn next_trade_responder(_state: &State, player: PlayerId) -> Option<PlayerId> {
    let next = player as usize + 1;
    if next >= PLAYER_COUNT {
        None
    } else {
        Some(next as u8)
    }
}

fn first_trade_responder(offering: PlayerId) -> Option<PlayerId> {
    for idx in 0..PLAYER_COUNT {
        if idx as u8 != offering {
            return Some(idx as u8);
        }
    }
    None
}

fn next_player(player: PlayerId, direction: i8) -> PlayerId {
    let count = PLAYER_COUNT as i8;
    let mut idx = player as i8 + direction;
    if idx < 0 {
        idx += count;
    }
    (idx % count) as u8
}

fn advance_turn(state: &mut State, direction: i8) {
    let next = next_player(state.turn_player, direction);
    state.turn_player = next;
    state.active_player = next;
    state.num_turns += 1;
}

fn clean_turn(state: &mut State, player: PlayerId) {
    state.has_played_dev[player as usize] = false;
    state.has_rolled[player as usize] = false;
    state.trade_offered_this_turn = false;
    for card in [
        DevCard::Knight,
        DevCard::Monopoly,
        DevCard::YearOfPlenty,
        DevCard::RoadBuilding,
    ] {
        let idx = card.as_index();
        state.dev_owned_at_start[player as usize][idx] =
            state.dev_cards_in_hand[player as usize][idx] > 0;
    }
}

fn player_settlement_count(state: &State, player: PlayerId) -> u8 {
    state
        .node_owner
        .iter()
        .zip(state.node_level.iter())
        .filter(|(owner, level)| **owner == player && **level == BuildingLevel::Settlement)
        .count() as u8
}

fn player_road_count(state: &State, player: PlayerId) -> u8 {
    state
        .edge_owner
        .iter()
        .filter(|owner| **owner == player)
        .count() as u8
}

fn total_settlement_count(state: &State) -> u8 {
    state
        .node_owner
        .iter()
        .zip(state.node_level.iter())
        .filter(|(_, level)| **level == BuildingLevel::Settlement)
        .count() as u8
}

pub(crate) fn player_resource_total(state: &State, player: PlayerId) -> u8 {
    state.player_resources[player as usize]
        .iter()
        .map(|value| *value as u32)
        .sum::<u32>() as u8
}

pub(crate) fn choose_discard(
    state: &State,
    player: PlayerId,
    rng: &mut impl RngCore,
) -> [u8; RESOURCE_COUNT] {
    random_discard_counts(&state.player_resources[player as usize], rng)
}

fn random_discard_counts(
    hand: &[u8; RESOURCE_COUNT],
    rng: &mut impl RngCore,
) -> [u8; RESOURCE_COUNT] {
    let total: usize = hand.iter().map(|count| *count as usize).sum();
    let to_discard = total / 2;
    if to_discard == 0 {
        return [0u8; RESOURCE_COUNT];
    }

    let mut deck = Vec::with_capacity(total);
    for resource in PYTHON_RESOURCE_ORDER {
        let count = hand[resource.as_index()] as usize;
        for _ in 0..count {
            deck.push(resource);
        }
    }

    shuffle_with_rng(&mut deck, rng);

    let mut counts = [0u8; RESOURCE_COUNT];
    for resource in deck.iter().take(to_discard) {
        counts[resource.as_index()] += 1;
    }
    counts
}

fn first_discarder(state: &State) -> PlayerId {
    for idx in 0..PLAYER_COUNT {
        if player_resource_total(state, idx as u8) > 7 {
            return idx as u8;
        }
    }
    state.turn_player
}

fn next_discarder_after(state: &State, current: PlayerId) -> Option<PlayerId> {
    for idx in (current as usize + 1)..PLAYER_COUNT {
        if player_resource_total(state, idx as u8) > 7 {
            return Some(idx as u8);
        }
    }
    None
}

#[cfg(feature = "legacy_robber")]
fn choose_robber_move(
    board: &crate::board::Board,
    state: &State,
    _road_state: &RoadState,
    _army_state: &ArmyState,
    player: PlayerId,
    _rng: &mut impl RngCore,
) -> (TileId, Option<PlayerId>, Option<Resource>) {
    choose_robber_move_legacy(board, state, player)
}

#[cfg(not(feature = "legacy_robber"))]
fn choose_robber_move(
    board: &crate::board::Board,
    state: &State,
    road_state: &RoadState,
    army_state: &ArmyState,
    player: PlayerId,
    rng: &mut impl RngCore,
) -> (TileId, Option<PlayerId>, Option<Resource>) {
    let leader = leader_by_public_vps(state, road_state, army_state, player);
    let best_tile = best_leader_robber_tile(board, state, leader, player, state.robber_tile);
    if let Some(tile) = best_tile {
        let victim = if player_has_building_on_tile(board, state, leader, tile)
            && player_resource_total(state, leader) > 0
        {
            Some(leader)
        } else {
            best_victim_on_tile(board, state, road_state, army_state, player, tile)
        };
        let stolen = victim.and_then(|victim_id| {
            random_resource_from_counts(rng, &state.player_resources[victim_id as usize])
        });
        return (tile, victim, stolen);
    }

    if let Some(tile) = first_non_self_robber_tile(board, state, player, state.robber_tile) {
        let victim = best_victim_on_tile(board, state, road_state, army_state, player, tile);
        let stolen = victim.and_then(|victim_id| {
            random_resource_from_counts(rng, &state.player_resources[victim_id as usize])
        });
        return (tile, victim, stolen);
    }

    (state.robber_tile, None, None)
}

#[cfg(feature = "legacy_robber")]
fn choose_robber_move_legacy(
    board: &crate::board::Board,
    state: &State,
    player: PlayerId,
) -> (TileId, Option<PlayerId>, Option<Resource>) {
    let mut target = state.robber_tile;
    for tile_id in 0..board.tile_numbers.len() {
        let tile = tile_id as u8;
        if tile != state.robber_tile {
            target = tile;
            break;
        }
    }

    let mut victim = None;
    for other in 0..PLAYER_COUNT {
        let other_id = other as u8;
        if other_id == player {
            continue;
        }
        if player_has_building_on_tile(board, state, other_id, target)
            && player_resource_total(state, other_id) > 0
        {
            victim = Some(other_id);
            break;
        }
    }

    let mut stolen = None;
    if let Some(victim_id) = victim {
        for resource in Resource::ALL {
            if state.player_resources[victim_id as usize][resource.as_index()] > 0 {
                stolen = Some(resource);
                break;
            }
        }
    }

    (target, victim, stolen)
}

#[cfg(not(feature = "legacy_robber"))]
fn leader_by_public_vps(
    state: &State,
    road_state: &RoadState,
    army_state: &ArmyState,
    player: PlayerId,
) -> PlayerId {
    let mut best_player = None;
    let mut best_score = f64::MIN;
    for other in 0..PLAYER_COUNT {
        let other_id = other as u8;
        if other_id == player {
            continue;
        }
        let score = estimated_public_vps(state, road_state, army_state, other_id);
        if best_player.is_none() || score > best_score {
            best_player = Some(other_id);
            best_score = score;
        }
    }
    best_player.unwrap_or_else(|| next_player(player, 1))
}

#[cfg(not(feature = "legacy_robber"))]
fn public_vps_for_player(
    state: &State,
    road_state: &RoadState,
    army_state: &ArmyState,
    player: PlayerId,
) -> u8 {
    let mut points = 0u8;
    for (idx, owner) in state.node_owner.iter().enumerate() {
        if *owner != player {
            continue;
        }
        points += match state.node_level[idx] {
            BuildingLevel::Settlement => 1,
            BuildingLevel::City => 2,
            BuildingLevel::Empty => 0,
        };
    }
    if road_state.owner() == Some(player) {
        points += 2;
    }
    if army_state.owner() == Some(player) {
        points += 2;
    }
    points
}

#[cfg(not(feature = "legacy_robber"))]
fn estimated_public_vps(
    state: &State,
    road_state: &RoadState,
    army_state: &ArmyState,
    player: PlayerId,
) -> f64 {
    let base = public_vps_for_player(state, road_state, army_state, player) as f64;
    let devs_in_hand: u8 = state.dev_cards_in_hand[player as usize]
        .iter()
        .copied()
        .sum();
    base + (devs_in_hand as f64 / 3.0)
}

#[cfg(not(feature = "legacy_robber"))]
fn best_leader_robber_tile(
    board: &crate::board::Board,
    state: &State,
    leader: PlayerId,
    player: PlayerId,
    current: TileId,
) -> Option<TileId> {
    let mut best_tile = None;
    let mut best_score = 0u32;
    for tile_id in 0..board.tile_numbers.len() {
        let tile = tile_id as TileId;
        if tile == current {
            continue;
        }
        if player_has_building_on_tile(board, state, player, tile) {
            continue;
        }
        let score = leader_tile_score(board, state, leader, tile);
        if score == 0 {
            continue;
        }
        if best_tile.is_none() || score > best_score {
            best_tile = Some(tile);
            best_score = score;
        }
    }
    best_tile
}

#[cfg(not(feature = "legacy_robber"))]
fn leader_tile_score(
    board: &crate::board::Board,
    state: &State,
    leader: PlayerId,
    tile: TileId,
) -> u32 {
    let number = match board.tile_numbers[tile as usize] {
        Some(number) => number,
        None => return 0,
    };
    let pip = pip_value(number) as u32;
    if pip == 0 {
        return 0;
    }
    let mut multiplier = 0u32;
    for node in board.tile_nodes[tile as usize] {
        if node == crate::types::INVALID_NODE {
            continue;
        }
        if state.node_owner[node as usize] != leader {
            continue;
        }
        multiplier += match state.node_level[node as usize] {
            BuildingLevel::Settlement => 1,
            BuildingLevel::City => 2,
            BuildingLevel::Empty => 0,
        };
    }
    pip * multiplier
}

#[cfg(not(feature = "legacy_robber"))]
fn pip_value(number: u8) -> u8 {
    match number {
        2 => 1,
        3 => 2,
        4 => 3,
        5 => 4,
        6 => 5,
        8 => 5,
        9 => 4,
        10 => 3,
        11 => 2,
        12 => 1,
        _ => 0,
    }
}

#[cfg(not(feature = "legacy_robber"))]
fn best_victim_on_tile(
    board: &crate::board::Board,
    state: &State,
    road_state: &RoadState,
    army_state: &ArmyState,
    player: PlayerId,
    tile: TileId,
) -> Option<PlayerId> {
    let mut best = None;
    let mut best_score = f64::MIN;
    for other in 0..PLAYER_COUNT {
        let other_id = other as u8;
        if other_id == player {
            continue;
        }
        if !player_has_building_on_tile(board, state, other_id, tile) {
            continue;
        }
        if player_resource_total(state, other_id) == 0 {
            continue;
        }
        let score = estimated_public_vps(state, road_state, army_state, other_id);
        if best.is_none() || score > best_score {
            best = Some(other_id);
            best_score = score;
        }
    }
    best
}

#[cfg(not(feature = "legacy_robber"))]
fn first_non_self_robber_tile(
    board: &crate::board::Board,
    state: &State,
    player: PlayerId,
    current: TileId,
) -> Option<TileId> {
    for tile_id in 0..board.tile_numbers.len() {
        let tile = tile_id as TileId;
        if tile == current {
            continue;
        }
        if player_has_building_on_tile(board, state, player, tile) {
            continue;
        }
        return Some(tile);
    }
    None
}

#[cfg(not(feature = "legacy_robber"))]
fn random_resource_from_counts(
    rng: &mut impl RngCore,
    counts: &[u8; RESOURCE_COUNT],
) -> Option<Resource> {
    let total: u64 = counts.iter().map(|count| *count as u64).sum();
    if total == 0 {
        return None;
    }
    let mut roll = next_u64_mod(rng, total);
    for resource in Resource::ALL {
        let amount = counts[resource.as_index()] as u64;
        if roll < amount {
            return Some(resource);
        }
        roll -= amount;
    }
    None
}

pub(crate) fn player_has_building_on_tile(
    board: &crate::board::Board,
    state: &State,
    player: PlayerId,
    tile: TileId,
) -> bool {
    for node in board.tile_nodes[tile as usize] {
        if node == crate::types::INVALID_NODE {
            continue;
        }
        if state.node_owner[node as usize] == player {
            return true;
        }
    }
    false
}

fn distribute_resources(board: &crate::board::Board, state: &mut State, roll: u8) {
    let mut total_by_resource = [0u8; RESOURCE_COUNT];
    let mut payouts = [[0u8; RESOURCE_COUNT]; PLAYER_COUNT];

    for tile_id in 0..board.tile_numbers.len() {
        if board.tile_numbers[tile_id] != Some(roll) {
            continue;
        }
        if tile_id as u8 == state.robber_tile {
            continue;
        }
        let resource = match board.tile_resources[tile_id] {
            Some(res) => res,
            None => continue,
        };
        for node in board.tile_nodes[tile_id] {
            if node == crate::types::INVALID_NODE {
                continue;
            }
            let owner = state.node_owner[node as usize];
            if owner == crate::types::NO_PLAYER {
                continue;
            }
            let amount = match state.node_level[node as usize] {
                BuildingLevel::Settlement => 1,
                BuildingLevel::City => 2,
                BuildingLevel::Empty => 0,
            };
            if amount == 0 {
                continue;
            }
            payouts[owner as usize][resource.as_index()] += amount;
            total_by_resource[resource.as_index()] += amount;
        }
    }

    for (idx, total) in total_by_resource.iter().enumerate() {
        if *total == 0 {
            continue;
        }
        if state.bank_resources[idx] < *total {
            for payout in payouts.iter_mut() {
                payout[idx] = 0;
            }
        }
    }

    let mut delta = Delta::default();
    for player in 0..PLAYER_COUNT {
        for (idx, amount) in payouts[player].iter().enumerate() {
            if *amount == 0 {
                continue;
            }
            let resource = Resource::from_index(idx).unwrap();
            state.adjust_resource(player as u8, resource, *amount as i8, &mut delta);
            state.adjust_bank(resource, -(*amount as i8), &mut delta);
        }
    }
}

fn distribute_resources_kernel(board: &crate::board::Board, state: &mut State, roll: u8) {
    let mut total_by_resource = [0u8; RESOURCE_COUNT];
    let mut payouts = [[0u8; RESOURCE_COUNT]; PLAYER_COUNT];

    for tile_id in 0..board.tile_numbers.len() {
        if board.tile_numbers[tile_id] != Some(roll) {
            continue;
        }
        if tile_id as u8 == state.robber_tile {
            continue;
        }
        let resource = match board.tile_resources[tile_id] {
            Some(res) => res,
            None => continue,
        };
        for node in board.tile_nodes[tile_id] {
            if node == crate::types::INVALID_NODE {
                continue;
            }
            let owner = state.node_owner[node as usize];
            if owner == crate::types::NO_PLAYER {
                continue;
            }
            let amount = match state.node_level[node as usize] {
                BuildingLevel::Settlement => 1,
                BuildingLevel::City => 2,
                BuildingLevel::Empty => 0,
            };
            if amount == 0 {
                continue;
            }
            payouts[owner as usize][resource.as_index()] += amount;
            total_by_resource[resource.as_index()] += amount;
        }
    }

    for (idx, total) in total_by_resource.iter().enumerate() {
        if *total == 0 {
            continue;
        }
        if state.bank_resources[idx] < *total {
            for payout in payouts.iter_mut() {
                payout[idx] = 0;
            }
        }
    }

    for player in 0..PLAYER_COUNT {
        for (idx, amount) in payouts[player].iter().enumerate() {
            if *amount == 0 {
                continue;
            }
            let resource = Resource::from_index(idx).unwrap();
            state.adjust_resource_kernel(player as u8, resource, *amount as i8);
            state.adjust_bank_kernel(resource, -(*amount as i8));
        }
    }
}

fn pay_cost(state: &mut State, player: PlayerId, cost: &[u8; RESOURCE_COUNT], delta: &mut Delta) {
    for (idx, amount) in cost.iter().enumerate() {
        if *amount == 0 {
            continue;
        }
        let resource = Resource::from_index(idx).unwrap();
        state.adjust_resource(player, resource, -(*amount as i8), delta);
        state.adjust_bank(resource, *amount as i8, delta);
    }
}

fn pay_cost_kernel(state: &mut State, player: PlayerId, cost: &[u8; RESOURCE_COUNT]) {
    for (idx, amount) in cost.iter().enumerate() {
        if *amount == 0 {
            continue;
        }
        let resource = Resource::from_index(idx).unwrap();
        state.adjust_resource_kernel(player, resource, -(*amount as i8));
        state.adjust_bank_kernel(resource, *amount as i8);
    }
}

pub(crate) fn can_play_dev(state: &State, player: PlayerId, card: DevCard) -> bool {
    if state.has_played_dev[player as usize] {
        return false;
    }
    let idx = card.as_index();
    if state.dev_cards_in_hand[player as usize][idx] == 0 {
        return false;
    }
    state.dev_owned_at_start[player as usize][idx]
}

pub(crate) fn trade_rate(
    board: &crate::board::Board,
    state: &State,
    player: PlayerId,
    offer: Resource,
) -> u8 {
    let mut rate = 4u8;
    let mut has_three_to_one = false;
    let mut has_two_to_one = false;

    for (node, port) in board.node_ports.iter().enumerate() {
        if *port == crate::types::PortType::None {
            continue;
        }
        if state.node_owner[node] != player {
            continue;
        }
        match port {
            crate::types::PortType::ThreeToOne => has_three_to_one = true,
            crate::types::PortType::Brick => has_two_to_one |= offer == Resource::Brick,
            crate::types::PortType::Lumber => has_two_to_one |= offer == Resource::Lumber,
            crate::types::PortType::Ore => has_two_to_one |= offer == Resource::Ore,
            crate::types::PortType::Grain => has_two_to_one |= offer == Resource::Grain,
            crate::types::PortType::Wool => has_two_to_one |= offer == Resource::Wool,
            crate::types::PortType::None => {}
        }
    }

    if has_two_to_one {
        rate = 2;
    } else if has_three_to_one {
        rate = 3;
    }
    rate
}

pub(crate) fn is_legal_build_road_free(
    board: &crate::board::Board,
    state: &State,
    player: PlayerId,
    edge: EdgeId,
) -> bool {
    if player_road_count(state, player) >= 15 {
        return false;
    }
    if edge as usize >= EDGE_COUNT {
        return false;
    }
    if state.edge_owner[edge as usize] != crate::types::NO_PLAYER {
        return false;
    }
    let nodes = board.edge_nodes[edge as usize];
    nodes
        .iter()
        .any(|node| state.road_components[player as usize].contains_node(*node))
}

fn update_largest_army(
    state: &State,
    army_state: &mut ArmyState,
    player: PlayerId,
    previous_owner: Option<PlayerId>,
    previous_size: u8,
) {
    let candidate_size = state.dev_cards_played[player as usize][DevCard::Knight.as_index()];

    if previous_owner == Some(player) {
        army_state.size = candidate_size;
        return;
    }

    if candidate_size < 3 {
        return;
    }

    match previous_owner {
        None => {
            army_state.owner = Some(player);
            army_state.size = candidate_size;
        }
        Some(_) => {
            if candidate_size > previous_size {
                army_state.owner = Some(player);
                army_state.size = candidate_size;
            }
        }
    }
}

#[inline]
fn node_mask(node: NodeId) -> u64 {
    if node as usize >= NODE_COUNT {
        return 0;
    }
    1u64 << node
}

fn is_enemy_node(state: &State, player: PlayerId, node: NodeId) -> bool {
    let owner = state.node_owner[node as usize];
    owner != crate::types::NO_PLAYER && owner != player
}

fn dfs_walk_mask(
    board: &crate::board::Board,
    state: &State,
    player: PlayerId,
    start: NodeId,
) -> u64 {
    let mut visited = [false; NODE_COUNT];
    let mut agenda = [0u8; NODE_COUNT];
    let mut agenda_len = 0usize;
    agenda[agenda_len] = start;
    agenda_len += 1;
    visited[start as usize] = true;
    let mut nodes = 0u64;

    while agenda_len > 0 {
        agenda_len -= 1;
        let node = agenda[agenda_len];
        let idx = node as usize;
        nodes |= node_mask(node);

        if is_enemy_node(state, player, node) {
            continue;
        }

        for edge in board.node_edges[idx] {
            if edge == crate::types::INVALID_EDGE {
                continue;
            }
            if state.edge_owner[edge as usize] != player {
                continue;
            }
            let edge_nodes = board.edge_nodes[edge as usize];
            let neighbor = if edge_nodes[0] == node {
                edge_nodes[1]
            } else {
                edge_nodes[0]
            };
            if !visited[neighbor as usize] {
                visited[neighbor as usize] = true;
                debug_assert!(agenda_len < NODE_COUNT);
                agenda[agenda_len] = neighbor;
                agenda_len += 1;
            }
        }
    }

    nodes
}

fn update_components_on_build_road(
    board: &crate::board::Board,
    state: &mut State,
    player: PlayerId,
    edge: EdgeId,
) {
    let nodes = board.edge_nodes[edge as usize];
    let a = nodes[0];
    let b = nodes[1];
    let enemy_a = is_enemy_node(state, player, a);
    let enemy_b = is_enemy_node(state, player, b);
    let components = &mut state.road_components[player as usize];

    let a_idx = components.component_index(a);
    let b_idx = components.component_index(b);

    if a_idx.is_none() && !enemy_a {
        if let Some(idx) = b_idx {
            components.add_node_to_component(idx, a);
        }
    } else if b_idx.is_none() && !enemy_b {
        if let Some(idx) = a_idx {
            components.add_node_to_component(idx, b);
        }
    } else if let (Some(ai), Some(bi)) = (a_idx, b_idx) {
        if ai != bi {
            components.merge_components(ai, bi);
        }
    }
}

fn update_components_on_build_settlement(
    board: &crate::board::Board,
    state: &mut State,
    player: PlayerId,
    node: NodeId,
) -> Vec<PlayerId> {
    let mut edge_counts = [0u8; PLAYER_COUNT];
    let mut first_edge = [INVALID_EDGE; PLAYER_COUNT];
    let mut second_edge = [INVALID_EDGE; PLAYER_COUNT];
    let mut plowed = Vec::new();

    for edge in board.node_edges[node as usize] {
        if edge == crate::types::INVALID_EDGE {
            continue;
        }
        let owner = state.edge_owner[edge as usize];
        if owner == crate::types::NO_PLAYER || owner == player {
            continue;
        }
        let owner_idx = owner as usize;
        if edge_counts[owner_idx] == 0 {
            first_edge[owner_idx] = edge;
        } else if edge_counts[owner_idx] == 1 {
            second_edge[owner_idx] = edge;
        }
        edge_counts[owner_idx] = edge_counts[owner_idx].saturating_add(1);
    }

    for owner_idx in 0..PLAYER_COUNT {
        if edge_counts[owner_idx] != 2 {
            continue;
        }
        let owner = owner_idx as PlayerId;
        plowed.push(owner);
        let edge_a = first_edge[owner_idx];
        let edge_b = second_edge[owner_idx];
        let nodes_a = board.edge_nodes[edge_a as usize];
        let nodes_b = board.edge_nodes[edge_b as usize];
        let a = if nodes_a[0] == node {
            nodes_a[1]
        } else {
            nodes_a[0]
        };
        let c = if nodes_b[0] == node {
            nodes_b[1]
        } else {
            nodes_b[0]
        };

        let a_nodes = dfs_walk_mask(board, state, owner, a);
        let c_nodes = dfs_walk_mask(board, state, owner, c);

        let components = &mut state.road_components[owner_idx];
        if let Some(idx) = components.component_index(node) {
            components.remove_component(idx);
        }
        components.push_mask(a_nodes);
        components.push_mask(c_nodes);
    }

    plowed
}

fn update_longest_road_after_build_road(
    board: &crate::board::Board,
    state: &State,
    road_state: &mut RoadState,
    player: PlayerId,
) {
    let candidate = longest_road_for_player(board, state, player);
    let idx = player as usize;
    if candidate > road_state.lengths[idx] {
        road_state.lengths[idx] = candidate;
    }

    if candidate >= 5 && candidate > road_state.length {
        road_state.length = candidate;
        road_state.owner = Some(player);
    }
}

fn update_longest_road_after_plow(
    board: &crate::board::Board,
    state: &State,
    road_state: &mut RoadState,
    plowed: &[PlayerId],
    previous_owner: Option<PlayerId>,
) {
    for player in plowed {
        let length = longest_road_for_player(board, state, *player);
        road_state.lengths[*player as usize] = length;
    }

    let (owner, length) = select_longest_road_owner(&road_state.lengths, previous_owner);
    road_state.length = length;
    road_state.owner = owner;
}

fn select_longest_road_owner(
    road_lengths: &[u8; PLAYER_COUNT],
    previous_owner: Option<PlayerId>,
) -> (Option<PlayerId>, u8) {
    let mut max_length = 0u8;
    for length in road_lengths {
        if *length > max_length {
            max_length = *length;
        }
    }

    if max_length == 0 {
        return (None, 0);
    }

    if let Some(owner) = previous_owner {
        if road_lengths[owner as usize] == max_length {
            return (Some(owner), max_length);
        }
    }

    for player in 0..PLAYER_COUNT {
        if road_lengths[player] == max_length {
            return (Some(player as u8), max_length);
        }
    }

    (None, max_length)
}

pub(crate) fn longest_road_for_player(
    board: &crate::board::Board,
    state: &State,
    player: PlayerId,
) -> u8 {
    let mut edges_by_node = [[INVALID_EDGE; 3]; NODE_COUNT];
    let mut edge_counts = [0u8; NODE_COUNT];
    for edge_id in 0..EDGE_COUNT {
        if state.edge_owner[edge_id] != player {
            continue;
        }
        let nodes = board.edge_nodes[edge_id];
        let a = nodes[0] as usize;
        let b = nodes[1] as usize;
        let a_idx = edge_counts[a] as usize;
        let b_idx = edge_counts[b] as usize;
        debug_assert!(a_idx < edges_by_node[a].len());
        debug_assert!(b_idx < edges_by_node[b].len());
        edges_by_node[a][a_idx] = edge_id as u8;
        edges_by_node[b][b_idx] = edge_id as u8;
        edge_counts[a] += 1;
        edge_counts[b] += 1;
    }

    let mut best = 0u8;
    for component in state.road_components[player as usize].iter_masks() {
        for node in ComponentNodeIter::from_mask(component) {
            let mut used = [false; EDGE_COUNT];
            let length = dfs_longest_path(
                board,
                state,
                player,
                node,
                &edges_by_node,
                &edge_counts,
                &mut used,
            );
            if length > best {
                best = length;
            }
        }
    }
    best
}

fn dfs_longest_path(
    board: &crate::board::Board,
    state: &State,
    player: PlayerId,
    node: NodeId,
    edges_by_node: &[[EdgeId; 3]; NODE_COUNT],
    edge_counts: &[u8; NODE_COUNT],
    used: &mut [bool; EDGE_COUNT],
) -> u8 {
    let mut best = 0u8;
    let node_idx = node as usize;
    for edge_slot in 0..edge_counts[node_idx] as usize {
        let edge = edges_by_node[node_idx][edge_slot];
        let edge_idx = edge as usize;
        if used[edge_idx] {
            continue;
        }
        used[edge_idx] = true;
        let nodes = board.edge_nodes[edge_idx];
        let next = if nodes[0] == node { nodes[1] } else { nodes[0] };
        let next_owner = state.node_owner[next as usize];
        if next_owner != crate::types::NO_PLAYER && next_owner != player {
            used[edge_idx] = false;
            continue;
        }
        let length =
            1 + dfs_longest_path(board, state, player, next, edges_by_node, edge_counts, used);
        if length > best {
            best = length;
        }
        used[edge_idx] = false;
    }
    best
}

fn player_points(
    state: &State,
    road_state: &RoadState,
    army_state: &ArmyState,
) -> [u8; PLAYER_COUNT] {
    let mut points = [0u8; PLAYER_COUNT];
    for (idx, owner) in state.node_owner.iter().enumerate() {
        if *owner == crate::types::NO_PLAYER {
            continue;
        }
        let level = state.node_level[idx];
        let add = match level {
            BuildingLevel::Settlement => 1,
            BuildingLevel::City => 2,
            BuildingLevel::Empty => 0,
        };
        points[*owner as usize] += add;
    }
    for player in 0..PLAYER_COUNT {
        points[player] += state.dev_cards_in_hand[player][DevCard::VictoryPoint.as_index()];
    }
    if let Some(owner) = road_state.owner {
        points[owner as usize] += 2;
    }
    if let Some(owner) = army_state.owner {
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
    for player in 1..PLAYER_COUNT {
        let score = points[player];
        if score > best_score {
            best_score = score;
            best_player = player;
        }
    }
    best_player as u8
}

fn log_action(log: &mut Option<&mut Vec<String>>, kind: &str, payload: Option<String>) {
    if let Some(log) = log.as_deref_mut() {
        let entry = match payload {
            Some(payload) => format!("{kind}:{payload}"),
            None => kind.to_string(),
        };
        log.push(entry);
    }
}

fn format_counts(counts: &[u8; RESOURCE_COUNT]) -> String {
    let mut out = String::new();
    for (idx, amount) in counts.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str(&amount.to_string());
    }
    out
}

fn format_robber_payload(
    tile: TileId,
    victim: Option<PlayerId>,
    resource: Option<Resource>,
) -> String {
    let victim_str = match victim {
        Some(id) => id.to_string(),
        None => "-".to_string(),
    };
    let resource_str = match resource {
        Some(res) => format_resource(res),
        None => "-".to_string(),
    };
    format!("tile={tile},victim={victim_str},res={resource_str}")
}

fn format_resource(resource: Resource) -> String {
    match resource {
        Resource::Brick => "BRICK".to_string(),
        Resource::Lumber => "WOOD".to_string(),
        Resource::Ore => "ORE".to_string(),
        Resource::Grain => "WHEAT".to_string(),
        Resource::Wool => "SHEEP".to_string(),
    }
}

fn format_dev_card(card: DevCard) -> String {
    match card {
        DevCard::Knight => "KNIGHT".to_string(),
        DevCard::YearOfPlenty => "YEAR_OF_PLENTY".to_string(),
        DevCard::Monopoly => "MONOPOLY".to_string(),
        DevCard::RoadBuilding => "ROAD_BUILDING".to_string(),
        DevCard::VictoryPoint => "VICTORY_POINT".to_string(),
    }
}

fn format_year_of_plenty_payload(first: Resource, second: Option<Resource>) -> String {
    let first_str = format_resource(first);
    let second_str = match second {
        Some(resource) => format_resource(resource),
        None => "-".to_string(),
    };
    format!("{first_str},{second_str}")
}

fn format_maritime_payload(offer: Resource, rate: u8, ask: Resource) -> String {
    format!(
        "offer={}:{};ask={}",
        format_resource(offer),
        rate,
        format_resource(ask)
    )
}

fn format_trade_payload(trade: [u8; RESOURCE_COUNT * 2]) -> String {
    let mut offer = [0u8; RESOURCE_COUNT];
    let mut ask = [0u8; RESOURCE_COUNT];
    offer.copy_from_slice(&trade[..RESOURCE_COUNT]);
    ask.copy_from_slice(&trade[RESOURCE_COUNT..]);
    format!(
        "offer={}|ask={}",
        format_counts(&offer),
        format_counts(&ask)
    )
}

fn format_confirm_trade_payload(trade: [u8; RESOURCE_COUNT * 2], partner: PlayerId) -> String {
    format!("{}|with={partner}", format_trade_payload(trade))
}
