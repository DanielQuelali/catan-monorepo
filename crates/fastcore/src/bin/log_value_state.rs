use fastcore::board::tile_coords;
use fastcore::board_config::board_from_json;
use fastcore::engine::{ArmyState, RoadState};
use fastcore::rng::{next_u64_mod, rng_for_stream, shuffle_with_rng};
use fastcore::state::State;
use fastcore::types::{
    BuildingLevel, DevCard, EdgeId, NodeId, PlayerId, Resource, PLAYER_COUNT,
    PYTHON_RESOURCE_ORDER, RESOURCE_COUNT,
};
use fastcore::value_player::{
    apply_value_action, FastValueFunctionPlayer, ValueAction, ValueActionKind,
};
use serde_json::Value;
use std::env;
use std::fs::File;
use std::io::BufReader;

const DEFAULT_STATE_PATH: &str = "../state_pre_last_settlement.json";
const DEFAULT_BOARD_PATH: &str = "../board_example.json";

fn load_actions(path: &str) -> (Vec<(String, String, Value)>, Vec<String>) {
    let file = File::open(path).expect("failed to open state json");
    let data: Value =
        serde_json::from_reader(BufReader::new(file)).expect("failed to parse state json");

    let colors = data["colors"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|value| value.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec!["RED", "BLUE", "ORANGE", "WHITE"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        });

    let actions = data["actions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| {
            let list = entry.as_array()?.to_vec();
            if list.len() != 3 {
                return None;
            }
            let color = list[0].as_str()?.to_string();
            let action_type = list[1].as_str()?.to_string();
            let payload = list[2].clone();
            Some((color, action_type, payload))
        })
        .collect::<Vec<_>>();

    (actions, colors)
}

fn color_to_player(colors: &[String], color: &str) -> PlayerId {
    colors.iter().position(|entry| entry == color).unwrap_or(0) as PlayerId
}

fn edge_id_for_nodes(board: &fastcore::board::Board, a: NodeId, b: NodeId) -> Option<EdgeId> {
    for (idx, nodes) in board.edge_nodes.iter().enumerate() {
        if (nodes[0] == a && nodes[1] == b) || (nodes[0] == b && nodes[1] == a) {
            return Some(idx as EdgeId);
        }
    }
    None
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

fn resource_name(resource: Resource) -> &'static str {
    match resource {
        Resource::Brick => "BRICK",
        Resource::Lumber => "WOOD",
        Resource::Ore => "ORE",
        Resource::Grain => "WHEAT",
        Resource::Wool => "SHEEP",
    }
}

fn dev_card_name(card: DevCard) -> &'static str {
    match card {
        DevCard::Knight => "KNIGHT",
        DevCard::YearOfPlenty => "YEAR_OF_PLENTY",
        DevCard::Monopoly => "MONOPOLY",
        DevCard::RoadBuilding => "ROAD_BUILDING",
        DevCard::VictoryPoint => "VICTORY_POINT",
    }
}

fn random_discard_sample(
    hand: &[u8; RESOURCE_COUNT],
    rng: &mut impl rand_core::RngCore,
) -> Vec<Resource> {
    let total: usize = hand.iter().map(|count| *count as usize).sum();
    let to_discard = total / 2;
    if to_discard == 0 {
        return Vec::new();
    }

    let mut deck = Vec::with_capacity(total);
    for resource in PYTHON_RESOURCE_ORDER {
        let count = hand[resource.as_index()] as usize;
        for _ in 0..count {
            deck.push(resource);
        }
    }
    shuffle_with_rng(&mut deck, rng);
    deck.into_iter().take(to_discard).collect()
}

fn draw_random_resource(
    rng: &mut impl rand_core::RngCore,
    counts: &[u8; RESOURCE_COUNT],
) -> Option<Resource> {
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

fn format_trade(offer: Resource, rate: u8, ask: Resource) -> String {
    let mut items = Vec::with_capacity(5);
    for _ in 0..rate {
        items.push(resource_name(offer).to_string());
    }
    for _ in rate..4 {
        items.push("None".to_string());
    }
    items.push(resource_name(ask).to_string());
    format!("({})", items.join(", "))
}

fn apply_action_with_log(
    board: &fastcore::board::Board,
    state: &mut State,
    road_state: &mut RoadState,
    army_state: &mut ArmyState,
    action: &ValueAction,
    rng: &mut (impl rand_core::RngCore + Clone),
) -> String {
    let color = color_name(action.player);
    let mut rng_clone = rng.clone();
    match action.kind {
        ValueActionKind::BuildSettlement(node) => {
            apply_value_action(board, state, road_state, army_state, action, rng);
            format!("{color} BUILD_SETTLEMENT {node}")
        }
        ValueActionKind::BuildRoad(edge) => {
            let nodes = board.edge_nodes[edge as usize];
            let (a, b) = if nodes[0] <= nodes[1] {
                (nodes[0], nodes[1])
            } else {
                (nodes[1], nodes[0])
            };
            apply_value_action(board, state, road_state, army_state, action, rng);
            format!("{color} BUILD_ROAD ({a}, {b})")
        }
        ValueActionKind::BuildCity(node) => {
            apply_value_action(board, state, road_state, army_state, action, rng);
            format!("{color} BUILD_CITY {node}")
        }
        ValueActionKind::Roll => {
            let roll = (
                next_u64_mod(&mut rng_clone, 6) + 1,
                next_u64_mod(&mut rng_clone, 6) + 1,
            );
            apply_value_action(board, state, road_state, army_state, action, rng);
            format!("{color} ROLL ({}, {})", roll.0, roll.1)
        }
        ValueActionKind::EndTurn => {
            apply_value_action(board, state, road_state, army_state, action, rng);
            format!("{color} END_TURN")
        }
        ValueActionKind::Discard(counts) => {
            let sample = counts.as_ref().map(|_| Vec::new()).unwrap_or_else(|| {
                random_discard_sample(
                    &state.player_resources[action.player as usize],
                    &mut rng_clone,
                )
            });
            apply_value_action(board, state, road_state, army_state, action, rng);
            if counts.is_some() {
                format!("{color} DISCARD")
            } else {
                let entries = sample
                    .iter()
                    .map(|res| resource_name(*res))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{color} DISCARD [{entries}]")
            }
        }
        ValueActionKind::MoveRobber {
            tile,
            victim,
            resource,
        } => {
            let stolen = if victim.is_some() && resource.is_none() {
                draw_random_resource(
                    &mut rng_clone,
                    &state.player_resources[victim.unwrap() as usize],
                )
            } else {
                resource
            };
            apply_value_action(board, state, road_state, army_state, action, rng);
            let (x, y, z) = tile_coords(tile);
            let victim_name = victim.map(color_name).unwrap_or("None");
            let res_name = stolen.map(resource_name).unwrap_or("None");
            format!("{color} MOVE_ROBBER ({x}, {y}, {z}) {victim_name} {res_name}")
        }
        ValueActionKind::PlayYearOfPlenty(first, second) => {
            apply_value_action(board, state, road_state, army_state, action, rng);
            let second = second.map(resource_name).unwrap_or("None");
            format!(
                "{color} PLAY_YEAR_OF_PLENTY ({}, {second})",
                resource_name(first)
            )
        }
        ValueActionKind::PlayMonopoly(resource) => {
            apply_value_action(board, state, road_state, army_state, action, rng);
            format!("{color} PLAY_MONOPOLY {}", resource_name(resource))
        }
        ValueActionKind::PlayKnight => {
            apply_value_action(board, state, road_state, army_state, action, rng);
            format!("{color} PLAY_KNIGHT_CARD")
        }
        ValueActionKind::PlayRoadBuilding => {
            apply_value_action(board, state, road_state, army_state, action, rng);
            format!("{color} PLAY_ROAD_BUILDING")
        }
        ValueActionKind::MaritimeTrade { offer, rate, ask } => {
            apply_value_action(board, state, road_state, army_state, action, rng);
            format!("{color} MARITIME_TRADE {}", format_trade(offer, rate, ask))
        }
        ValueActionKind::BuyDevCard => {
            let card = state.dev_deck.last().copied().unwrap_or(DevCard::Knight);
            apply_value_action(board, state, road_state, army_state, action, rng);
            format!("{color} BUY_DEVELOPMENT_CARD {}", dev_card_name(card))
        }
        ValueActionKind::AcceptTrade => {
            apply_value_action(board, state, road_state, army_state, action, rng);
            format!("{color} ACCEPT_TRADE")
        }
        ValueActionKind::RejectTrade => {
            apply_value_action(board, state, road_state, army_state, action, rng);
            format!("{color} REJECT_TRADE")
        }
        ValueActionKind::ConfirmTrade(partner) => {
            apply_value_action(board, state, road_state, army_state, action, rng);
            format!("{color} CONFIRM_TRADE {}", color_name(partner))
        }
        ValueActionKind::CancelTrade => {
            apply_value_action(board, state, road_state, army_state, action, rng);
            format!("{color} CANCEL_TRADE")
        }
    }
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
        points[player] += state.dev_cards_in_hand[player][DevCard::VictoryPoint.as_index()];
    }
    if let Some(owner) = road_state.owner() {
        points[owner as usize] += 2;
    }
    if army_state.size() >= 3 {
        if let Some(owner) = army_state.owner() {
            points[owner as usize] += 2;
        }
    }
    points
}

fn has_winner(state: &State, road_state: &RoadState, army_state: &ArmyState) -> bool {
    player_points(state, road_state, army_state)
        .iter()
        .any(|score| *score >= 10)
}

fn emit_line(show_turns: bool, turns: u32, line: &str) {
    if show_turns {
        println!("turn={turns} {line}");
    } else {
        println!("{line}");
    }
}

fn main() {
    let mut args = env::args().skip(1);
    let state_path = args
        .next()
        .unwrap_or_else(|| DEFAULT_STATE_PATH.to_string());
    let board_path = args
        .next()
        .unwrap_or_else(|| DEFAULT_BOARD_PATH.to_string());
    let seed: u64 = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let steps: usize = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(50);
    let mut show_turns = false;
    let mut stop_on_win = false;
    let mut log_initial = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--with-turns" => show_turns = true,
            "--stop-on-win" => stop_on_win = true,
            "--log-initial" => log_initial = true,
            _ => {}
        }
    }

    let (actions, colors) = load_actions(&state_path);
    let board = board_from_json(&board_path).expect("failed to load board json");

    let mut rng = rng_for_stream(seed, 0);
    let mut state = State::new_with_rng_and_board(&mut rng, &board);
    let mut road_state = RoadState::empty();
    let mut army_state = ArmyState::empty();

    for (color, action_type, payload) in actions {
        let player = color_to_player(&colors, &color);
        if state.active_player != player {
            state.active_player = player;
            state.turn_player = player;
        }
        let value_action = match action_type.as_str() {
            "BUILD_SETTLEMENT" => {
                let node = payload.as_u64().unwrap() as NodeId;
                ValueAction {
                    player,
                    kind: ValueActionKind::BuildSettlement(node),
                }
            }
            "BUILD_ROAD" => {
                let pair = payload.as_array().unwrap();
                let a = pair[0].as_u64().unwrap() as NodeId;
                let b = pair[1].as_u64().unwrap() as NodeId;
                let edge = edge_id_for_nodes(&board, a, b).expect("edge id not found");
                ValueAction {
                    player,
                    kind: ValueActionKind::BuildRoad(edge),
                }
            }
            other => panic!("unsupported action type: {other}"),
        };
        let line = apply_action_with_log(
            &board,
            &mut state,
            &mut road_state,
            &mut army_state,
            &value_action,
            &mut rng,
        );
        if log_initial {
            emit_line(show_turns, state.num_turns, &line);
        }
        if stop_on_win && has_winner(&state, &road_state, &army_state) {
            return;
        }
    }

    let players = [
        FastValueFunctionPlayer::new(None, None),
        FastValueFunctionPlayer::new(None, None),
        FastValueFunctionPlayer::new(None, None),
        FastValueFunctionPlayer::new(None, None),
    ];

    for _ in 0..steps {
        let player = state.active_player as usize;
        let action = players[player].decide(&board, &state, &road_state, &army_state, &mut rng);
        let line = apply_action_with_log(
            &board,
            &mut state,
            &mut road_state,
            &mut army_state,
            &action,
            &mut rng,
        );
        emit_line(show_turns, state.num_turns, &line);
        if stop_on_win && has_winner(&state, &road_state, &army_state) {
            break;
        }
    }
}
