use fastcore::board_config::board_from_json;
use fastcore::engine::{ArmyState, RoadState};
use fastcore::rng::rng_for_stream;
use fastcore::state::State;
use fastcore::types::{BuildingLevel, EdgeId, NodeId, PlayerId, PLAYER_COUNT};
use fastcore::value_player::{
    apply_value_action, generate_playable_actions, FastValueFunctionPlayer, ValueAction,
    ValueActionKind,
};
use serde_json::Value;
use std::cmp::Ordering;
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

fn parse_args() -> (
    String,
    String,
    u64,
    usize,
    bool,
    Vec<NodeId>,
    Vec<EdgeId>,
    usize,
) {
    let mut state_path = DEFAULT_STATE_PATH.to_string();
    let mut board_path = DEFAULT_BOARD_PATH.to_string();
    let mut seed = 1u64;
    let mut limit = 10usize;
    let mut print_state = false;
    let mut debug_nodes = Vec::new();
    let mut debug_edges = Vec::new();
    let mut advance = 0usize;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => {
                if let Some(value) = args.next() {
                    state_path = value;
                }
            }
            "--board" => {
                if let Some(value) = args.next() {
                    board_path = value;
                }
            }
            "--seed" => {
                if let Some(value) = args.next() {
                    seed = value.parse().unwrap_or(1);
                }
            }
            "--limit" => {
                if let Some(value) = args.next() {
                    limit = value.parse().unwrap_or(10);
                }
            }
            "--print-state" => {
                print_state = true;
            }
            "--debug-node" => {
                if let Some(value) = args.next() {
                    for item in value.split(',') {
                        if let Ok(node) = item.trim().parse::<u8>() {
                            debug_nodes.push(node);
                        }
                    }
                }
            }
            "--debug-edge" => {
                if let Some(value) = args.next() {
                    for item in value.split(',') {
                        if let Ok(edge) = item.trim().parse::<u8>() {
                            debug_edges.push(edge);
                        }
                    }
                }
            }
            "--advance" => {
                if let Some(value) = args.next() {
                    advance = value.parse().unwrap_or(0);
                }
            }
            _ => {}
        }
    }

    (
        state_path,
        board_path,
        seed,
        limit,
        print_state,
        debug_nodes,
        debug_edges,
        advance,
    )
}

fn main() {
    let (state_path, board_path, seed, limit, print_state, debug_nodes, debug_edges, advance) =
        parse_args();
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
        apply_value_action(
            &board,
            &mut state,
            &mut road_state,
            &mut army_state,
            &value_action,
            &mut rng,
        );
    }

    let players = [
        FastValueFunctionPlayer::new(None, None),
        FastValueFunctionPlayer::new(None, None),
        FastValueFunctionPlayer::new(None, None),
        FastValueFunctionPlayer::new(None, None),
    ];

    if advance > 0 {
        for _ in 0..advance {
            let current = state.active_player as usize;
            let action =
                players[current].decide(&board, &state, &road_state, &army_state, &mut rng);
            apply_value_action(
                &board,
                &mut state,
                &mut road_state,
                &mut army_state,
                &action,
                &mut rng,
            );
        }
    }

    if print_state {
        println!(
            "STATE prompt={:?} active_player={} turn_player={} is_road_building={} free_roads={}",
            state.current_prompt,
            state.active_player,
            state.turn_player,
            state.is_road_building,
            state.free_roads_available
        );
        for player in 0..PLAYER_COUNT {
            let mut settlements = Vec::new();
            let mut cities = Vec::new();
            for node in 0..state.node_owner.len() {
                if state.node_owner[node] != player as u8 {
                    continue;
                }
                match state.node_level[node] {
                    BuildingLevel::Settlement => settlements.push(node as u8),
                    BuildingLevel::City => cities.push(node as u8),
                    BuildingLevel::Empty => {}
                }
            }
            let mut roads = Vec::new();
            for (edge_idx, owner) in state.edge_owner.iter().enumerate() {
                if *owner != player as u8 {
                    continue;
                }
                let nodes = board.edge_nodes[edge_idx];
                roads.push((nodes[0], nodes[1]));
            }
            println!(
                "PLAYER {player} settlements={:?} cities={:?} roads={:?}",
                settlements, cities, roads
            );
            println!(
                "PLAYER {player} dev_in_hand={:?} dev_owned_start={:?} played_dev={}",
                state.dev_cards_in_hand[player],
                state.dev_owned_at_start[player],
                state.has_played_dev[player]
            );
            println!(
                "PLAYER {player} dev_played={:?}",
                state.dev_cards_played[player]
            );
        }
        println!("ROAD_STATE {:?}", road_state);
        println!("ARMY_STATE {:?}", army_state);
    }

    let player = state.active_player;
    let value_player = FastValueFunctionPlayer::new(None, None);
    let mut actions = generate_playable_actions(&board, &state, player);
    let playable_nodes: Vec<NodeId> = actions
        .iter()
        .filter_map(|action| match action.kind {
            ValueActionKind::BuildSettlement(node) => Some(node),
            _ => None,
        })
        .collect();
    let playable_edges: Vec<EdgeId> = actions
        .iter()
        .filter_map(|action| match action.kind {
            ValueActionKind::BuildRoad(edge) => Some(edge),
            _ => None,
        })
        .collect();

    if !debug_nodes.is_empty() {
        for node in debug_nodes {
            if !playable_nodes.contains(&node) {
                println!("NODE {node} not playable");
                continue;
            }
            let action = ValueAction {
                player,
                kind: ValueActionKind::BuildSettlement(node),
            };
            let mut state_copy = state.clone();
            let mut road_copy = road_state;
            let mut army_copy = army_state;
            apply_value_action(
                &board,
                &mut state_copy,
                &mut road_copy,
                &mut army_copy,
                &action,
                &mut rng,
            );
            let components =
                value_player.value_components(&board, &state_copy, &road_copy, &army_copy, player);
            println!("NODE {node}");
            println!("production {}", components.production);
            println!("enemy_production {}", components.enemy_production);
            println!("reachable_0 {}", components.reachable_0);
            println!("reachable_1 {}", components.reachable_1);
            println!("reachable_2 {}", components.reachable_2);
            println!("reachable_3 {}", components.reachable_3);
            println!("hand_synergy {}", components.hand_synergy);
            println!("num_buildable_nodes {}", components.num_buildable_nodes);
            println!("num_tiles {}", components.num_tiles);
            println!("num_in_hand {}", components.num_in_hand);
            println!("discard_penalty {}", components.discard_penalty);
            println!("longest_road_length {}", components.longest_road_length);
            println!("longest_road_factor {}", components.longest_road_factor);
            println!("city_trade_gap {}", components.city_trade_gap);
            println!("port_trade_value {}", components.port_trade_value);
            println!("devs_bought {}", components.devs_bought);
            println!("devs_in_hand {}", components.devs_in_hand);
            println!("vps {}", components.vps);
            println!(
                "hand_counts {:?}",
                state_copy.player_resources[player as usize]
            );
            let hand_counts = state_copy.player_resources[player as usize];
            let wheat = hand_counts[fastcore::types::Resource::Grain.as_index()];
            let ore = hand_counts[fastcore::types::Resource::Ore.as_index()];
            let sheep = hand_counts[fastcore::types::Resource::Wool.as_index()];
            let wood = hand_counts[fastcore::types::Resource::Lumber.as_index()];
            let brick = hand_counts[fastcore::types::Resource::Brick.as_index()];
            let distance_to_city =
                ((2_i32 - wheat as i32).max(0) as f64 + (3_i32 - ore as i32).max(0) as f64) / 5.0;
            let term_wheat = (1_i32 - wheat as i32).max(0) as f64;
            let term_sheep = (1_i32 - sheep as i32).max(0) as f64;
            let term_brick = (1_i32 - brick as i32).max(0) as f64;
            let term_wood = (1_i32 - wood as i32).max(0) as f64;
            let distance_to_settlement = (term_wheat + term_sheep + term_brick + term_wood) / 4.0;
            let hand_synergy = (2.0 - distance_to_city - distance_to_settlement) / 2.0;
            println!(
                "hand_synergy_recalc {} wheat={} ore={} sheep={} wood={} brick={} dist_city={} dist_settle={} terms=({}, {}, {}, {})",
                hand_synergy,
                wheat,
                ore,
                sheep,
                wood,
                brick,
                distance_to_city,
                distance_to_settlement,
                term_wheat,
                term_sheep,
                term_brick,
                term_wood
            );
            println!("prod_by_res {:?}", components.prod_by_res);
            println!(
                "reachable_zero_by_res {:?}",
                components.reachable_zero_by_res
            );
            println!("total {}", components.total);
            println!("---");
        }
    }

    if !debug_edges.is_empty() {
        for edge in debug_edges {
            let nodes = board.edge_nodes[edge as usize];
            let node_states = nodes.map(|node| {
                let owner = state.node_owner[node as usize];
                let mut friendly_road = false;
                for edge_id in board.node_edges[node as usize] {
                    if edge_id == fastcore::types::INVALID_EDGE {
                        continue;
                    }
                    if state.edge_owner[edge_id as usize] == player {
                        friendly_road = true;
                        break;
                    }
                }
                (node, owner, friendly_road)
            });
            let legal = fastcore::rules::is_legal_build_road(&board, &state, player, edge);
            if !playable_edges.contains(&edge) {
                println!(
                    "EDGE {edge} ({}, {}) not playable legal={legal} nodes={:?}",
                    nodes[0], nodes[1], node_states
                );
                continue;
            }
            let action = ValueAction {
                player,
                kind: ValueActionKind::BuildRoad(edge),
            };
            let mut state_copy = state.clone();
            let mut road_copy = road_state;
            let mut army_copy = army_state;
            apply_value_action(
                &board,
                &mut state_copy,
                &mut road_copy,
                &mut army_copy,
                &action,
                &mut rng,
            );
            let value = value_player.value(&board, &state_copy, &road_copy, &army_copy, player);
            println!(
                "EDGE {edge} ({}, {}) value {} legal={} nodes={:?}",
                nodes[0], nodes[1], value, legal, node_states
            );
        }
    }
    actions.sort_by(|a, b| action_sort_key(a).cmp(&action_sort_key(b)));

    let mut scored = Vec::with_capacity(actions.len());
    for action in actions {
        let mut state_copy = state.clone();
        let mut road_copy = road_state;
        let mut army_copy = army_state;
        apply_value_action(
            &board,
            &mut state_copy,
            &mut road_copy,
            &mut army_copy,
            &action,
            &mut rng,
        );
        let value = value_player.value(&board, &state_copy, &road_copy, &army_copy, player);
        scored.push((action, value));
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    for (action, value) in scored.into_iter().take(limit) {
        match action.kind {
            ValueActionKind::BuildSettlement(node) => {
                println!("{node} {value}");
            }
            _ => {
                println!("{:?} {value}", action.kind);
            }
        }
    }
}

fn action_sort_key(action: &ValueAction) -> (u8, String, String) {
    let kind = match action.kind {
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
    };
    (action.player, kind.to_string(), action_payload(action))
}

fn action_payload(action: &ValueAction) -> String {
    match action.kind {
        ValueActionKind::BuildSettlement(node) | ValueActionKind::BuildCity(node) => {
            format!("{node}")
        }
        ValueActionKind::BuildRoad(edge) => format!("{edge}"),
        ValueActionKind::Discard(_) => String::new(),
        ValueActionKind::MoveRobber {
            tile,
            victim,
            resource,
        } => {
            format!("{tile:?}{victim:?}{resource:?}")
        }
        ValueActionKind::PlayYearOfPlenty(first, second) => {
            format!("{first:?}{second:?}")
        }
        ValueActionKind::PlayMonopoly(resource) => format!("{resource:?}"),
        ValueActionKind::MaritimeTrade { offer, rate, ask } => {
            format!("{offer:?}{rate}{ask:?}")
        }
        ValueActionKind::ConfirmTrade(partner) => format!("{partner}"),
        _ => String::new(),
    }
}
