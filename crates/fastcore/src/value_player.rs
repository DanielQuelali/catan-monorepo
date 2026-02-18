use crate::board::Board;
use crate::board_data::{EDGE_NODES, TILE_COORDS};
use crate::engine::{
    apply_accept_trade, apply_build_city, apply_build_road, apply_build_settlement,
    apply_cancel_trade, apply_confirm_trade, apply_discard, apply_end_turn, apply_initial_road,
    apply_initial_settlement, apply_knight, apply_maritime_trade, apply_monopoly,
    apply_move_robber, apply_reject_trade, apply_road_building, apply_roll, apply_year_of_plenty,
    buy_dev_card, can_accept_trade, can_buy_dev_card, can_play_dev, is_legal_build_road_free,
    player_has_building_on_tile, player_resource_total, trade_rate, ArmyState, RoadState,
};
use crate::rng::{next_u64_mod, roll_die, shuffle_with_rng};
use crate::rules;
use crate::state::State;
use crate::types::{
    ActionPrompt, BuildingLevel, DevCard, EdgeId, NodeId, PlayerId, PortType, Resource, TileId,
    EDGE_COUNT, INVALID_EDGE, INVALID_NODE, INVALID_TILE, NODE_COUNT, NO_PLAYER, PLAYER_COUNT,
    PYTHON_RESOURCE_ORDER, RESOURCE_COUNT, TILE_COUNT,
};
use rand_core::RngCore;
use std::cmp::Ordering;
use std::sync::OnceLock;

const RESOURCE_ORDER: [Resource; RESOURCE_COUNT] = [
    Resource::Lumber,
    Resource::Brick,
    Resource::Wool,
    Resource::Grain,
    Resource::Ore,
];

const DICE_PROBAS: [f64; 13] = [
    0.0,
    0.0,
    1.0 / 36.0,
    2.0 / 36.0,
    3.0 / 36.0,
    4.0 / 36.0,
    5.0 / 36.0,
    6.0 / 36.0,
    5.0 / 36.0,
    4.0 / 36.0,
    3.0 / 36.0,
    2.0 / 36.0,
    1.0 / 36.0,
];

const TRANSLATE_VARIETY: f64 = 4.0;
const PROBA_POINT: f64 = 2.778 / 100.0;
const ENDGAME_GAP_WEIGHT: f64 = 50_000_000.0;

static NODE_PRODUCTION: OnceLock<[[f64; RESOURCE_COUNT]; NODE_COUNT]> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct ValueWeights {
    pub public_vps: f64,
    pub production: f64,
    pub enemy_production: f64,
    pub num_tiles: f64,
    pub reachable_production_0: f64,
    pub reachable_production_1: f64,
    pub reachable_production_2: f64,
    pub reachable_production_3: f64,
    pub buildable_nodes: f64,
    pub longest_road: f64,
    pub hand_synergy: f64,
    pub hand_resources: f64,
    pub discard_penalty: f64,
    pub devs_bought: f64,
    pub devs_in_hand_penalty: f64,
    pub army_size: f64,
    pub city_trade_gap: f64,
    pub port_trade: f64,
    pub port_trade_cap: Option<f64>,
}

impl Default for ValueWeights {
    fn default() -> Self {
        Self {
            public_vps: 300000000000000.0,
            production: 100_000_000.0,
            enemy_production: -100_000_000.0,
            num_tiles: 1.0,
            reachable_production_0: 10_000.0,
            reachable_production_1: 2_000.0,
            reachable_production_2: 10.0,
            reachable_production_3: 10.0,
            buildable_nodes: -1000.0,
            longest_road: 10.0,
            hand_synergy: 100.0,
            hand_resources: 1.0,
            discard_penalty: -5_000_000.0,
            devs_bought: 10_000_000.0,
            devs_in_hand_penalty: -20_000.0,
            army_size: 10.1,
            city_trade_gap: 10.0,
            port_trade: 100.0,
            port_trade_cap: Some(0.0),
        }
    }
}

impl ValueWeights {
    pub fn contender() -> Self {
        Self {
            public_vps: 300000000000001.94,
            production: 100000002.04188395,
            enemy_production: -99999998.03389844,
            num_tiles: 2.91440418,
            reachable_production_0: 2.03820085,
            reachable_production_1: 10002.018773150001,
            reachable_production_2: 0.0,
            reachable_production_3: 0.0,
            buildable_nodes: 1001.86278466,
            longest_road: 12.127388499999999,
            hand_synergy: 102.40606877,
            hand_resources: 2.43644327,
            discard_penalty: -3.00141993,
            devs_bought: 0.0,
            devs_in_hand_penalty: 0.0,
            army_size: 12.93844622,
            city_trade_gap: 0.0,
            port_trade: 0.0,
            port_trade_cap: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueActionKind {
    BuildSettlement(NodeId),
    BuildRoad(EdgeId),
    BuildCity(NodeId),
    Roll,
    EndTurn,
    Discard(Option<[u8; RESOURCE_COUNT]>),
    MoveRobber {
        tile: TileId,
        victim: Option<PlayerId>,
        resource: Option<Resource>,
    },
    PlayYearOfPlenty(Resource, Option<Resource>),
    PlayMonopoly(Resource),
    PlayKnight,
    PlayRoadBuilding,
    MaritimeTrade {
        offer: Resource,
        rate: u8,
        ask: Resource,
    },
    BuyDevCard,
    AcceptTrade,
    RejectTrade,
    ConfirmTrade(PlayerId),
    CancelTrade,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueAction {
    pub player: PlayerId,
    pub kind: ValueActionKind,
}

#[derive(Clone, Debug)]
pub struct FastValueFunctionPlayer {
    weights: ValueWeights,
    epsilon: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct ValueComponents {
    pub production: f64,
    pub enemy_production: f64,
    pub reachable_0: f64,
    pub reachable_1: f64,
    pub reachable_2: f64,
    pub reachable_3: f64,
    pub hand_synergy: f64,
    pub num_buildable_nodes: f64,
    pub num_tiles: f64,
    pub num_in_hand: f64,
    pub discard_penalty: f64,
    pub longest_road_length: f64,
    pub longest_road_factor: f64,
    pub city_trade_gap: f64,
    pub port_trade_value: f64,
    pub devs_bought: f64,
    pub devs_in_hand: f64,
    pub vps: f64,
    pub total: f64,
    pub prod_by_res: [f64; RESOURCE_COUNT],
    pub reachable_zero_by_res: [f64; RESOURCE_COUNT],
}

impl FastValueFunctionPlayer {
    pub fn new(weights: Option<ValueWeights>, epsilon: Option<f64>) -> Self {
        Self {
            weights: weights.unwrap_or_default(),
            epsilon,
        }
    }

    pub fn contender(epsilon: Option<f64>) -> Self {
        Self {
            weights: ValueWeights::contender(),
            epsilon,
        }
    }

    pub fn decide(
        &self,
        board: &Board,
        state: &State,
        road_state: &RoadState,
        army_state: &ArmyState,
        rng: &mut impl RngCore,
    ) -> ValueAction {
        let player = state.active_player;
        let mut actions = generate_playable_actions(board, state, player);
        if actions.is_empty() {
            return ValueAction {
                player,
                kind: ValueActionKind::EndTurn,
            };
        }

        if state.current_prompt == ActionPrompt::MoveRobber && !cfg!(feature = "legacy_robber") {
            if let Some(action) =
                leader_robber_action(board, state, road_state, army_state, player, &actions)
            {
                return action;
            }
            let non_blocking: Vec<ValueAction> = actions
                .iter()
                .cloned()
                .filter(|action| match action.kind {
                    ValueActionKind::MoveRobber { tile, .. } => {
                        !player_has_building_on_tile(board, state, player, tile)
                    }
                    _ => true,
                })
                .collect();
            if !non_blocking.is_empty() {
                actions = non_blocking;
            }
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

        if let Some(epsilon) = self.epsilon {
            let roll = rng.next_u64() as f64 / (u64::MAX as f64 + 1.0);
            if roll < epsilon {
                let idx = next_u64_mod(rng, actions.len() as u64) as usize;
                return actions[idx].clone();
            }
        }

        actions.sort_by(|a, b| action_sort_key(a).cmp(&action_sort_key(b)));

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
            let value = self.value(board, &state_copy, &road_copy, &army_copy, player);
            if value > best_value {
                best_value = value;
                best_action = action;
            }
        }

        best_action
    }

    pub fn value(
        &self,
        board: &Board,
        state: &State,
        road_state: &RoadState,
        army_state: &ArmyState,
        player: PlayerId,
    ) -> f64 {
        self.value_components(board, state, road_state, army_state, player)
            .total
    }

    pub fn value_components(
        &self,
        board: &Board,
        state: &State,
        road_state: &RoadState,
        army_state: &ArmyState,
        player: PlayerId,
    ) -> ValueComponents {
        let weights = &self.weights;
        let public_vps_value = public_vps(state, road_state, army_state, player) as f64;
        let endgame = cfg!(feature = "endgame_gap") && public_vps_value >= 7.0;
        let endgame_multiplier = if endgame { 0.2 } else { 1.0 };

        let prod_by_res = production_by_resource(board, state, player, true);
        let production = value_production(&prod_by_res, true);

        let mut enemy_production = 0.0;
        let order = iter_players(player);
        for enemy in order.iter().skip(1) {
            let enemy_prod = production_by_resource(board, state, *enemy, true);
            enemy_production += value_production(&enemy_prod, false);
        }

        let reachability = reachability_no_enemy(board, state, player, 3);
        let reachable_at_zero = reachability[0];
        let reachable_at_one = reachability[1];
        let reachable_at_two = reachability[2];
        let reachable_at_three = reachability[3];

        let hand_counts = state.player_resources[player as usize];
        let wheat_in_hand = hand_counts[Resource::Grain.as_index()];
        let ore_in_hand = hand_counts[Resource::Ore.as_index()];
        let sheep_in_hand = hand_counts[Resource::Wool.as_index()];
        let wood_in_hand = hand_counts[Resource::Lumber.as_index()];
        let brick_in_hand = hand_counts[Resource::Brick.as_index()];

        let distance_to_city = ((2_i32 - wheat_in_hand as i32).max(0) as f64
            + (3_i32 - ore_in_hand as i32).max(0) as f64)
            / 5.0;
        let distance_to_settlement = ((1_i32 - wheat_in_hand as i32).max(0) as f64
            + (1_i32 - sheep_in_hand as i32).max(0) as f64
            + (1_i32 - brick_in_hand as i32).max(0) as f64
            + (1_i32 - wood_in_hand as i32).max(0) as f64)
            / 4.0;
        let hand_synergy = (2.0 - distance_to_city - distance_to_settlement) / 2.0;

        let num_in_hand = player_resource_total(state, player) as f64;
        let discard_penalty = if num_in_hand > 7.0 {
            (num_in_hand - 7.0) * weights.discard_penalty
        } else {
            0.0
        };

        let num_tiles = owned_tile_count(board, state, player) as f64;

        let num_buildable_nodes = buildable_node_count(board, state, player) as f64;
        let longest_road_factor = if num_buildable_nodes == 0.0 {
            weights.longest_road
        } else {
            0.1
        };
        let longest_road_factor = if endgame {
            longest_road_factor * endgame_multiplier
        } else {
            longest_road_factor
        };
        let longest_road_length = road_state.length_for_player(player) as f64;

        let (player_ports, has_three_to_one) = player_ports(board, state, player);
        let ore_needed = (3_i32 - ore_in_hand as i32).max(0) as f64;
        let wheat_needed = (2_i32 - wheat_in_hand as i32).max(0) as f64;
        let missing_city_cards = ore_needed + wheat_needed;

        let mut tradable_cover = 0.0;
        for resource in RESOURCE_ORDER {
            let in_hand = hand_counts[resource.as_index()] as f64;
            let surplus = match resource {
                Resource::Ore => (in_hand - 3.0).max(0.0),
                Resource::Grain => (in_hand - 2.0).max(0.0),
                _ => in_hand,
            };
            let mut rate = 4.0;
            if has_three_to_one {
                rate = 3.0;
            }
            if player_ports.contains(&resource) {
                rate = 2.0;
            }
            tradable_cover += surplus / rate;
        }

        let city_trade_gap = ((missing_city_cards - tradable_cover) / 5.0).max(0.0);

        let base_trade_rate = if has_three_to_one {
            1.0 / 3.0
        } else {
            1.0 / 4.0
        };
        let mut port_trade_value = 0.0;
        for resource in player_ports.iter() {
            let res_idx = resource.as_index();
            let upgrade_gain =
                (prod_by_res[res_idx] + reachable_at_zero[res_idx]) * (0.5 - base_trade_rate);
            if upgrade_gain > 0.0 {
                port_trade_value += upgrade_gain;
            }
        }
        if let Some(cap) = weights.port_trade_cap {
            port_trade_value = port_trade_value.min(cap);
        }

        let devs_in_hand = state.dev_cards_in_hand[player as usize]
            .iter()
            .map(|count| *count as f64)
            .sum::<f64>();
        let devs_played = state.dev_cards_played[player as usize]
            .iter()
            .map(|count| *count as f64)
            .sum::<f64>();
        let devs_bought = devs_in_hand + devs_played;

        let knights_played = state.dev_cards_played[player as usize][DevCard::Knight.as_index()];

        let vps = public_vps_value;
        let build_gap = distance_to_city + distance_to_settlement;
        let endgame_gap_term = if endgame {
            -build_gap * ENDGAME_GAP_WEIGHT
        } else {
            0.0
        };

        let total = vps * weights.public_vps
            + production * weights.production * endgame_multiplier
            + enemy_production * weights.enemy_production * endgame_multiplier
            + sum_resources(&reachable_at_zero)
                * weights.reachable_production_0
                * endgame_multiplier
            + sum_resources(&reachable_at_one)
                * weights.reachable_production_1
                * endgame_multiplier
            + sum_resources(&reachable_at_two)
                * weights.reachable_production_2
                * endgame_multiplier
            + sum_resources(&reachable_at_three)
                * weights.reachable_production_3
                * endgame_multiplier
            - city_trade_gap * weights.city_trade_gap
            + hand_synergy * weights.hand_synergy
            + num_buildable_nodes * weights.buildable_nodes * endgame_multiplier
            + num_tiles * weights.num_tiles * endgame_multiplier
            + num_in_hand * weights.hand_resources
            + discard_penalty
            + longest_road_length * longest_road_factor
            + (knights_played as f64) * weights.army_size
            + port_trade_value * weights.port_trade * endgame_multiplier
            + devs_bought * weights.devs_bought
            + devs_in_hand * weights.devs_in_hand_penalty
            + endgame_gap_term;

        ValueComponents {
            production,
            enemy_production,
            reachable_0: sum_resources(&reachable_at_zero),
            reachable_1: sum_resources(&reachable_at_one),
            reachable_2: sum_resources(&reachable_at_two),
            reachable_3: sum_resources(&reachable_at_three),
            hand_synergy,
            num_buildable_nodes,
            num_tiles,
            num_in_hand,
            discard_penalty,
            longest_road_length,
            longest_road_factor,
            city_trade_gap,
            port_trade_value,
            devs_bought,
            devs_in_hand,
            vps,
            total,
            prod_by_res,
            reachable_zero_by_res: reachable_at_zero,
        }
    }
}

pub fn generate_playable_actions(
    board: &Board,
    state: &State,
    player: PlayerId,
) -> Vec<ValueAction> {
    let mut actions = Vec::new();

    match state.current_prompt {
        ActionPrompt::BuildInitialSettlement => {
            for node in 0..NODE_COUNT {
                let node_id = node as NodeId;
                if rules::is_legal_initial_settlement(board, state, node_id) {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::BuildSettlement(node_id),
                    });
                }
            }
        }
        ActionPrompt::BuildInitialRoad => {
            let anchor = state.last_initial_settlement[player as usize];
            if anchor != INVALID_NODE {
                for edge in board.node_edges[anchor as usize] {
                    if edge == INVALID_EDGE {
                        continue;
                    }
                    if rules::is_legal_initial_road(board, state, player, edge, anchor) {
                        actions.push(ValueAction {
                            player,
                            kind: ValueActionKind::BuildRoad(edge),
                        });
                    }
                }
            }
        }
        ActionPrompt::Discard => {
            actions.push(ValueAction {
                player,
                kind: ValueActionKind::Discard(None),
            });
        }
        ActionPrompt::MoveRobber => {
            for tile in 0..TILE_COUNT {
                let tile_id = tile as TileId;
                if tile_id == state.robber_tile {
                    continue;
                }
                let mut victims = Vec::new();
                for other in 0..PLAYER_COUNT {
                    let other_id = other as PlayerId;
                    if other_id == player {
                        continue;
                    }
                    if player_has_building_on_tile(board, state, other_id, tile_id)
                        && player_resource_total(state, other_id) > 0
                    {
                        victims.push(other_id);
                    }
                }
                if victims.is_empty() {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::MoveRobber {
                            tile: tile_id,
                            victim: None,
                            resource: None,
                        },
                    });
                } else {
                    for victim in victims {
                        actions.push(ValueAction {
                            player,
                            kind: ValueActionKind::MoveRobber {
                                tile: tile_id,
                                victim: Some(victim),
                                resource: None,
                            },
                        });
                    }
                }
            }
        }
        ActionPrompt::DecideTrade => {
            actions.push(ValueAction {
                player,
                kind: ValueActionKind::RejectTrade,
            });
            if can_accept_trade(state, player) {
                actions.push(ValueAction {
                    player,
                    kind: ValueActionKind::AcceptTrade,
                });
            }
        }
        ActionPrompt::DecideAcceptees => {
            actions.push(ValueAction {
                player,
                kind: ValueActionKind::CancelTrade,
            });
            for (idx, accepted) in state.acceptees.iter().enumerate() {
                if *accepted {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::ConfirmTrade(idx as PlayerId),
                    });
                }
            }
        }
        ActionPrompt::PlayTurn => {
            if state.is_road_building {
                for edge in 0..EDGE_COUNT {
                    let edge_id = edge as EdgeId;
                    if is_legal_build_road_free(board, state, player, edge_id) {
                        actions.push(ValueAction {
                            player,
                            kind: ValueActionKind::BuildRoad(edge_id),
                        });
                    }
                }
                return actions;
            }

            if can_play_dev(state, player, DevCard::YearOfPlenty) {
                actions.extend(year_of_plenty_actions(state, player));
            }
            if can_play_dev(state, player, DevCard::Monopoly) {
                for resource in RESOURCE_ORDER {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::PlayMonopoly(resource),
                    });
                }
            }
            if can_play_dev(state, player, DevCard::Knight) {
                actions.push(ValueAction {
                    player,
                    kind: ValueActionKind::PlayKnight,
                });
            }
            if can_play_dev(state, player, DevCard::RoadBuilding) {
                if (0..EDGE_COUNT)
                    .map(|edge| edge as EdgeId)
                    .any(|edge| is_legal_build_road_free(board, state, player, edge))
                {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::PlayRoadBuilding,
                    });
                }
            }

            if !state.has_rolled[player as usize] {
                actions.push(ValueAction {
                    player,
                    kind: ValueActionKind::Roll,
                });
                return actions;
            }

            actions.push(ValueAction {
                player,
                kind: ValueActionKind::EndTurn,
            });

            for node in 0..NODE_COUNT {
                let node_id = node as NodeId;
                if rules::is_legal_build_city(board, state, player, node_id) {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::BuildCity(node_id),
                    });
                }
            }
            for node in 0..NODE_COUNT {
                let node_id = node as NodeId;
                if rules::is_legal_build_settlement(board, state, player, node_id) {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::BuildSettlement(node_id),
                    });
                }
            }
            for edge in 0..EDGE_COUNT {
                let edge_id = edge as EdgeId;
                if rules::is_legal_build_road(board, state, player, edge_id) {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::BuildRoad(edge_id),
                    });
                }
            }

            if can_buy_dev_card(state, player) {
                actions.push(ValueAction {
                    player,
                    kind: ValueActionKind::BuyDevCard,
                });
            }

            actions.extend(maritime_trade_actions(board, state, player));
        }
    }

    actions
}

pub fn apply_value_action(
    board: &Board,
    state: &mut State,
    road_state: &mut RoadState,
    army_state: &mut ArmyState,
    action: &ValueAction,
    rng: &mut impl RngCore,
) {
    match action.kind {
        ValueActionKind::BuildSettlement(node) => {
            if state.is_initial_build_phase
                && state.current_prompt == ActionPrompt::BuildInitialSettlement
            {
                apply_initial_settlement(board, state, road_state, node);
            } else {
                apply_build_settlement(board, state, road_state, action.player, node);
            }
        }
        ValueActionKind::BuildRoad(edge) => {
            if state.is_initial_build_phase
                && state.current_prompt == ActionPrompt::BuildInitialRoad
            {
                apply_initial_road(board, state, road_state, edge);
            } else if state.is_road_building {
                apply_build_road(board, state, road_state, action.player, edge, true);
                state.free_roads_available = state.free_roads_available.saturating_sub(1);
                if state.free_roads_available == 0 || !has_free_road(board, state, action.player) {
                    state.is_road_building = false;
                    state.free_roads_available = 0;
                }
            } else {
                apply_build_road(board, state, road_state, action.player, edge, false);
            }
        }
        ValueActionKind::BuildCity(node) => {
            apply_build_city(board, state, action.player, node);
        }
        ValueActionKind::Roll => {
            let roll = (roll_die(rng) as u32, roll_die(rng) as u32);
            apply_roll(board, state, roll);
        }
        ValueActionKind::EndTurn => {
            apply_end_turn(state, action.player);
        }
        ValueActionKind::Discard(counts) => {
            let discard = counts.unwrap_or_else(|| {
                random_discard_counts(&state.player_resources[action.player as usize], rng)
            });
            apply_discard(state, action.player, &discard);
        }
        ValueActionKind::MoveRobber {
            tile,
            victim,
            resource,
        } => {
            let stolen = if victim.is_some() && resource.is_none() {
                draw_random_resource(rng, &state.player_resources[victim.unwrap() as usize])
            } else {
                resource
            };
            apply_move_robber(state, tile, victim, stolen);
        }
        ValueActionKind::PlayYearOfPlenty(first, second) => {
            apply_year_of_plenty(state, first, second);
        }
        ValueActionKind::PlayMonopoly(resource) => {
            apply_monopoly(state, resource);
        }
        ValueActionKind::PlayKnight => {
            apply_knight(state, army_state);
            state.current_prompt = ActionPrompt::MoveRobber;
            state.is_moving_robber = true;
        }
        ValueActionKind::PlayRoadBuilding => {
            apply_road_building(state);
            state.current_prompt = ActionPrompt::PlayTurn;
        }
        ValueActionKind::MaritimeTrade { offer, rate, ask } => {
            apply_maritime_trade(state, action.player, offer, rate, ask);
        }
        ValueActionKind::BuyDevCard => {
            let _ = buy_dev_card(state, action.player);
        }
        ValueActionKind::AcceptTrade => {
            apply_accept_trade(state, action.player);
        }
        ValueActionKind::RejectTrade => {
            apply_reject_trade(state, action.player);
        }
        ValueActionKind::ConfirmTrade(partner) => {
            apply_confirm_trade(state, partner);
        }
        ValueActionKind::CancelTrade => {
            apply_cancel_trade(state);
        }
    }
}

fn year_of_plenty_actions(state: &State, player: PlayerId) -> Vec<ValueAction> {
    let mut options: Vec<(Resource, Option<Resource>)> = Vec::new();
    let mut push_unique = |entry: (Resource, Option<Resource>)| {
        if !options.contains(&entry) {
            options.push(entry);
        }
    };
    for (i, first) in RESOURCE_ORDER.iter().enumerate() {
        for second in RESOURCE_ORDER.iter().skip(i) {
            let first_ok = state.bank_resources[first.as_index()] > 0;
            let second_ok = state.bank_resources[second.as_index()] > 0;
            if first == second {
                if state.bank_resources[first.as_index()] >= 2 {
                    push_unique((*first, Some(*second)));
                } else if first_ok {
                    push_unique((*first, None));
                }
            } else if first_ok && second_ok {
                push_unique((*first, Some(*second)));
            } else {
                if first_ok {
                    push_unique((*first, None));
                }
                if second_ok {
                    push_unique((*second, None));
                }
            }
        }
    }

    options
        .into_iter()
        .map(|(first, second)| ValueAction {
            player,
            kind: ValueActionKind::PlayYearOfPlenty(first, second),
        })
        .collect()
}

fn maritime_trade_actions(board: &Board, state: &State, player: PlayerId) -> Vec<ValueAction> {
    let mut actions = Vec::new();
    for offer in RESOURCE_ORDER {
        let rate = trade_rate(board, state, player, offer);
        if state.player_resources[player as usize][offer.as_index()] < rate {
            continue;
        }
        for ask in RESOURCE_ORDER {
            if ask == offer {
                continue;
            }
            if state.bank_resources[ask.as_index()] > 0 {
                actions.push(ValueAction {
                    player,
                    kind: ValueActionKind::MaritimeTrade { offer, rate, ask },
                });
            }
        }
    }
    actions
}

fn has_free_road(board: &Board, state: &State, player: PlayerId) -> bool {
    (0..EDGE_COUNT).any(|edge| is_legal_build_road_free(board, state, player, edge as EdgeId))
}

fn action_sort_key(action: &ValueAction) -> ActionSortKey {
    ActionSortKey {
        kind: action_kind_name(&action.kind),
        payload: action_payload_key(&action.kind),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActionSortKey {
    kind: &'static str,
    payload: Vec<SortValue>,
}

impl Ord for ActionSortKey {
    fn cmp(&self, other: &Self) -> Ordering {
        let kind_cmp = self.kind.cmp(other.kind);
        if kind_cmp != Ordering::Equal {
            return kind_cmp;
        }
        self.payload.cmp(&other.payload)
    }
}

impl PartialOrd for ActionSortKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SortValue {
    None,
    Int(i32),
    Str(&'static str),
}

impl Ord for SortValue {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (SortValue::None, SortValue::None) => Ordering::Equal,
            (SortValue::None, _) => Ordering::Less,
            (_, SortValue::None) => Ordering::Greater,
            (SortValue::Int(a), SortValue::Int(b)) => a.cmp(b),
            (SortValue::Str(a), SortValue::Str(b)) => a.cmp(b),
            (SortValue::Int(_), SortValue::Str(_)) => Ordering::Less,
            (SortValue::Str(_), SortValue::Int(_)) => Ordering::Greater,
        }
    }
}

impl PartialOrd for SortValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn action_kind_name(kind: &ValueActionKind) -> &'static str {
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

fn action_payload_key(kind: &ValueActionKind) -> Vec<SortValue> {
    match kind {
        ValueActionKind::BuildSettlement(node) | ValueActionKind::BuildCity(node) => {
            vec![SortValue::Int(*node as i32)]
        }
        ValueActionKind::BuildRoad(edge) => {
            let nodes = EDGE_NODES[*edge as usize];
            let (a, b) = if nodes[0] <= nodes[1] {
                (nodes[0], nodes[1])
            } else {
                (nodes[1], nodes[0])
            };
            vec![SortValue::Int(a as i32), SortValue::Int(b as i32)]
        }
        ValueActionKind::Discard(counts) => counts
            .as_ref()
            .map(|counts| {
                counts
                    .iter()
                    .map(|count| SortValue::Int(*count as i32))
                    .collect()
            })
            .unwrap_or_default(),
        ValueActionKind::MoveRobber {
            tile,
            victim,
            resource,
        } => {
            let (x, y, z) = tile_coords(*tile);
            vec![
                SortValue::Int(x as i32),
                SortValue::Int(y as i32),
                SortValue::Int(z as i32),
                victim
                    .map(|id| SortValue::Str(color_name(id)))
                    .unwrap_or(SortValue::None),
                resource
                    .map(|res| SortValue::Str(resource_name(res)))
                    .unwrap_or(SortValue::None),
            ]
        }
        ValueActionKind::PlayYearOfPlenty(first, second) => {
            if let Some(second) = second {
                vec![
                    SortValue::Str(resource_name(*first)),
                    SortValue::Str(resource_name(*second)),
                ]
            } else {
                vec![SortValue::Str(resource_name(*first))]
            }
        }
        ValueActionKind::PlayMonopoly(resource) => vec![SortValue::Str(resource_name(*resource))],
        ValueActionKind::MaritimeTrade { offer, rate, ask } => vec![
            SortValue::Str(resource_name(*offer)),
            SortValue::Int(*rate as i32),
            SortValue::Str(resource_name(*ask)),
        ],
        ValueActionKind::ConfirmTrade(partner) => {
            vec![SortValue::Str(color_name(*partner))]
        }
        _ => Vec::new(),
    }
}

fn resource_name(resource: Resource) -> &'static str {
    match resource {
        Resource::Brick => "BRICK",
        Resource::Lumber => "WOOD",
        Resource::Ore => "ORE",
        Resource::Grain => "WHEAT",
        Resource::Wool => "SHEEP",
    }
}

fn color_name(player: PlayerId) -> &'static str {
    match player {
        0 => "RED",
        1 => "BLUE",
        2 => "ORANGE",
        3 => "WHITE",
        _ => "UNKNOWN",
    }
}

fn tile_coords(tile: TileId) -> (i8, i8, i8) {
    if (tile as usize) < TILE_COORDS.len() {
        TILE_COORDS[tile as usize]
    } else {
        (0, 0, 0)
    }
}

fn iter_players(p0: PlayerId) -> Vec<PlayerId> {
    let mut order = Vec::with_capacity(PLAYER_COUNT);
    for offset in 0..PLAYER_COUNT {
        let idx = ((p0 as usize + offset) % PLAYER_COUNT) as PlayerId;
        order.push(idx);
    }
    order
}

fn sum_resources(values: &[f64; RESOURCE_COUNT]) -> f64 {
    values.iter().sum()
}

fn production_by_resource(
    board: &Board,
    state: &State,
    player: PlayerId,
    consider_robber: bool,
) -> [f64; RESOURCE_COUNT] {
    let table = node_production_table(board);
    let mut output = [0.0; RESOURCE_COUNT];
    let mut robbed = [false; NODE_COUNT];

    if consider_robber {
        for node in board.tile_nodes[state.robber_tile as usize] {
            if node != INVALID_NODE {
                robbed[node as usize] = true;
            }
        }
    }

    for node in 0..NODE_COUNT {
        let node_id = node as NodeId;
        if state.node_owner[node] != player {
            continue;
        }
        if consider_robber && robbed[node] {
            continue;
        }
        let multiplier = match state.node_level[node] {
            BuildingLevel::Settlement => 1.0,
            BuildingLevel::City => 2.0,
            BuildingLevel::Empty => 0.0,
        };
        if multiplier == 0.0 {
            continue;
        }
        for idx in 0..RESOURCE_COUNT {
            output[idx] += multiplier * table[node_id as usize][idx];
        }
    }

    output
}

fn value_production(values: &[f64; RESOURCE_COUNT], include_variety: bool) -> f64 {
    let mut sum = 0.0;
    let mut variety = 0.0;
    let order = [
        Resource::Grain,
        Resource::Ore,
        Resource::Wool,
        Resource::Lumber,
        Resource::Brick,
    ];
    for resource in order {
        let value = values[resource.as_index()];
        sum += value;
        if value != 0.0 {
            variety += 1.0;
        }
    }
    if include_variety {
        sum + variety * TRANSLATE_VARIETY * PROBA_POINT
    } else {
        sum
    }
}

fn node_production_table(board: &Board) -> &[[f64; RESOURCE_COUNT]; NODE_COUNT] {
    NODE_PRODUCTION.get_or_init(|| {
        let mut table = [[0.0; RESOURCE_COUNT]; NODE_COUNT];
        for node in 0..NODE_COUNT {
            for tile in board.node_tiles[node] {
                if tile == INVALID_TILE {
                    continue;
                }
                let tile_idx = tile as usize;
                let resource = match board.tile_resources[tile_idx] {
                    Some(resource) => resource,
                    None => continue,
                };
                let number = match board.tile_numbers[tile_idx] {
                    Some(number) => number,
                    None => continue,
                };
                table[node][resource.as_index()] += number_probability(number);
            }
        }
        table
    })
}

fn number_probability(number: u8) -> f64 {
    if number as usize >= DICE_PROBAS.len() {
        return 0.0;
    }
    DICE_PROBAS[number as usize]
}

fn reachability_no_enemy(
    board: &Board,
    state: &State,
    p0: PlayerId,
    levels: usize,
) -> Vec<[f64; RESOURCE_COUNT]> {
    let board_buildable = global_buildable_nodes(board, state);
    let mut outputs = vec![[0.0; RESOURCE_COUNT]; levels + 1];

    let owned_or_buildable = owned_or_buildable_nodes(state, p0, &board_buildable);
    let zero_nodes = player_zero_nodes(board, state, p0);
    let mut owned_zero = zero_nodes;
    for node in 0..NODE_COUNT {
        if state.node_owner[node] != p0 && state.node_owner[node] != NO_PLAYER {
            owned_zero[node] = false;
        }
    }
    let mut production = [0.0; RESOURCE_COUNT];
    accumulate_production(board, owned_or_buildable, owned_zero, &mut production);
    outputs[0] = production;

    let enemy_nodes = enemy_nodes_mask(state, p0);
    let enemy_roads = enemy_roads_mask(state, p0);
    let level_nodes =
        reachable_nodes_by_level(board, &zero_nodes, &enemy_nodes, &enemy_roads, levels);

    for (level_idx, nodes_mask) in level_nodes.into_iter().enumerate() {
        let mut level_mask = nodes_mask;
        for node in 0..NODE_COUNT {
            if enemy_nodes[node] {
                level_mask[node] = false;
            }
        }
        let mut production = [0.0; RESOURCE_COUNT];
        accumulate_production(board, owned_or_buildable, level_mask, &mut production);
        outputs[level_idx + 1] = production;
    }

    outputs
}

fn owned_or_buildable_nodes(
    state: &State,
    player: PlayerId,
    buildable: &[bool; NODE_COUNT],
) -> [bool; NODE_COUNT] {
    let mut mask = [false; NODE_COUNT];
    for node in 0..NODE_COUNT {
        if buildable[node] || state.node_owner[node] == player {
            mask[node] = true;
        }
    }
    mask
}

fn global_buildable_nodes(board: &Board, state: &State) -> [bool; NODE_COUNT] {
    let mut mask = [false; NODE_COUNT];
    for node in 0..NODE_COUNT {
        if state.node_owner[node] != NO_PLAYER {
            continue;
        }
        if node_is_adjacent_occupied(board, state, node as NodeId) {
            continue;
        }
        mask[node] = true;
    }
    mask
}

fn node_is_adjacent_occupied(board: &Board, state: &State, node: NodeId) -> bool {
    for edge in board.node_edges[node as usize] {
        if edge == INVALID_EDGE {
            continue;
        }
        let nodes = board.edge_nodes[edge as usize];
        let other = if nodes[0] == node { nodes[1] } else { nodes[0] };
        if state.node_owner[other as usize] != NO_PLAYER {
            return true;
        }
    }
    false
}

fn player_zero_nodes(board: &Board, state: &State, player: PlayerId) -> [bool; NODE_COUNT] {
    let mut visited = [false; NODE_COUNT];
    let mut stack = Vec::new();

    for node in 0..NODE_COUNT {
        if state.node_owner[node] == player {
            stack.push(node as NodeId);
        }
    }

    for edge in 0..EDGE_COUNT {
        if state.edge_owner[edge] == player {
            let nodes = board.edge_nodes[edge];
            stack.push(nodes[0]);
            stack.push(nodes[1]);
        }
    }

    while let Some(node) = stack.pop() {
        let idx = node as usize;
        if visited[idx] {
            continue;
        }
        visited[idx] = true;
        if state.node_owner[idx] != NO_PLAYER && state.node_owner[idx] != player {
            continue;
        }
        for edge in board.node_edges[idx] {
            if edge == INVALID_EDGE {
                continue;
            }
            if state.edge_owner[edge as usize] != player {
                continue;
            }
            let nodes = board.edge_nodes[edge as usize];
            let other = if nodes[0] == node { nodes[1] } else { nodes[0] };
            if !visited[other as usize] {
                stack.push(other);
            }
        }
    }

    visited
}

fn enemy_nodes_mask(state: &State, player: PlayerId) -> [bool; NODE_COUNT] {
    let mut mask = [false; NODE_COUNT];
    for node in 0..NODE_COUNT {
        let owner = state.node_owner[node];
        if owner != NO_PLAYER && owner != player {
            mask[node] = true;
        }
    }
    mask
}

fn enemy_roads_mask(state: &State, player: PlayerId) -> [bool; EDGE_COUNT] {
    let mut mask = [false; EDGE_COUNT];
    for edge in 0..EDGE_COUNT {
        let owner = state.edge_owner[edge];
        if owner != NO_PLAYER && owner != player {
            mask[edge] = true;
        }
    }
    mask
}

fn reachable_nodes_by_level(
    board: &Board,
    zero_nodes: &[bool; NODE_COUNT],
    enemy_nodes: &[bool; NODE_COUNT],
    enemy_roads: &[bool; EDGE_COUNT],
    levels: usize,
) -> Vec<[bool; NODE_COUNT]> {
    let mut results = Vec::with_capacity(levels);
    let mut level_nodes = *zero_nodes;
    let mut last_layer = level_nodes;

    for _ in 0..levels {
        let mut next_nodes = level_nodes;
        for node in 0..NODE_COUNT {
            if !last_layer[node] {
                continue;
            }
            if enemy_nodes[node] {
                continue;
            }
            for edge in board.node_edges[node] {
                if edge == INVALID_EDGE {
                    continue;
                }
                if enemy_roads[edge as usize] {
                    continue;
                }
                let nodes = board.edge_nodes[edge as usize];
                let other = if nodes[0] == node as u8 {
                    nodes[1]
                } else {
                    nodes[0]
                };
                next_nodes[other as usize] = true;
            }
        }
        level_nodes = next_nodes;
        last_layer = level_nodes;
        results.push(level_nodes);
    }

    results
}

fn accumulate_production(
    board: &Board,
    owned_or_buildable: [bool; NODE_COUNT],
    nodes_mask: [bool; NODE_COUNT],
    output: &mut [f64; RESOURCE_COUNT],
) {
    let table = node_production_table(board);
    for node in 0..NODE_COUNT {
        if !owned_or_buildable[node] || !nodes_mask[node] {
            continue;
        }
        for idx in 0..RESOURCE_COUNT {
            output[idx] += table[node][idx];
        }
    }
}

fn owned_tile_count(board: &Board, state: &State, player: PlayerId) -> usize {
    let mut owned = [false; TILE_COUNT];
    for node in 0..NODE_COUNT {
        if state.node_owner[node] != player {
            continue;
        }
        for tile in board.node_tiles[node] {
            if tile == INVALID_TILE {
                continue;
            }
            owned[tile as usize] = true;
        }
    }
    owned.iter().filter(|value| **value).count()
}

fn buildable_node_count(board: &Board, state: &State, player: PlayerId) -> usize {
    let buildable = global_buildable_nodes(board, state);
    let zero_nodes = player_zero_nodes(board, state, player);
    buildable
        .iter()
        .zip(zero_nodes.iter())
        .filter(|(buildable, zero)| **buildable && **zero)
        .count()
}

fn player_ports(board: &Board, state: &State, player: PlayerId) -> (Vec<Resource>, bool) {
    let mut ports = Vec::new();
    let mut has_three_to_one = false;
    for node in 0..NODE_COUNT {
        if state.node_owner[node] != player {
            continue;
        }
        match board.node_ports[node] {
            PortType::None => {}
            PortType::ThreeToOne => has_three_to_one = true,
            PortType::Brick => ports.push(Resource::Brick),
            PortType::Lumber => ports.push(Resource::Lumber),
            PortType::Ore => ports.push(Resource::Ore),
            PortType::Grain => ports.push(Resource::Grain),
            PortType::Wool => ports.push(Resource::Wool),
        }
    }
    (ports, has_three_to_one)
}

fn public_vps(
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
    if let Some(best) = best_player {
        best
    } else {
        if player == 0 {
            1
        } else {
            0
        }
    }
}

fn estimated_public_vps(
    state: &State,
    road_state: &RoadState,
    army_state: &ArmyState,
    player: PlayerId,
) -> f64 {
    let base = public_vps(state, road_state, army_state, player) as f64;
    let devs_in_hand: u8 = state.dev_cards_in_hand[player as usize]
        .iter()
        .copied()
        .sum();
    base + (devs_in_hand as f64 / 3.0)
}

fn leader_robber_action(
    board: &Board,
    state: &State,
    road_state: &RoadState,
    army_state: &ArmyState,
    player: PlayerId,
    actions: &[ValueAction],
) -> Option<ValueAction> {
    let leader = leader_by_public_vps(state, road_state, army_state, player);
    let tile = best_leader_robber_tile(board, state, leader, player, state.robber_tile)?;
    if let Some(action) = actions.iter().find(|action| {
        matches!(
            action.kind,
            ValueActionKind::MoveRobber {
                tile: t,
                victim: Some(v),
                resource: None
            } if t == tile && v == leader
        )
    }) {
        return Some(action.clone());
    }
    let victim = best_victim_on_tile(board, state, road_state, army_state, player, tile)?;
    actions
        .iter()
        .find(|action| {
            matches!(
                action.kind,
                ValueActionKind::MoveRobber {
                    tile: t,
                    victim: Some(v),
                    resource: None
                } if t == tile && v == victim
            )
        })
        .cloned()
}

fn best_leader_robber_tile(
    board: &Board,
    state: &State,
    leader: PlayerId,
    player: PlayerId,
    current: TileId,
) -> Option<TileId> {
    let mut best_tile = None;
    let mut best_score = 0.0_f64;
    for tile_id in 0..TILE_COUNT {
        let tile = tile_id as TileId;
        if tile == current {
            continue;
        }
        if player_has_building_on_tile(board, state, player, tile) {
            continue;
        }
        let score = leader_tile_score(board, state, leader, tile);
        if score <= 0.0 {
            continue;
        }
        if best_tile.is_none() || score > best_score {
            best_tile = Some(tile);
            best_score = score;
        }
    }
    best_tile
}

fn leader_tile_score(board: &Board, state: &State, leader: PlayerId, tile: TileId) -> f64 {
    let number = match board.tile_numbers[tile as usize] {
        Some(number) => number,
        None => return 0.0,
    };
    let base = number_probability(number);
    if base <= 0.0 {
        return 0.0;
    }
    let mut multiplier = 0.0;
    for node in board.tile_nodes[tile as usize] {
        if node == INVALID_NODE {
            continue;
        }
        if state.node_owner[node as usize] != leader {
            continue;
        }
        multiplier += match state.node_level[node as usize] {
            BuildingLevel::Settlement => 1.0,
            BuildingLevel::City => 2.0,
            BuildingLevel::Empty => 0.0,
        };
    }
    base * multiplier
}

fn best_victim_on_tile(
    board: &Board,
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

fn draw_random_resource(rng: &mut impl RngCore, counts: &[u8; RESOURCE_COUNT]) -> Option<Resource> {
    let total: u64 = counts.iter().map(|count| *count as u64).sum();
    if total == 0 {
        return None;
    }
    let mut roll = next_u64_mod(rng, total);
    for resource in PYTHON_RESOURCE_ORDER {
        let amount = counts[resource.as_index()] as u64;
        if roll < amount {
            return Some(resource);
        }
        roll -= amount;
    }
    None
}
