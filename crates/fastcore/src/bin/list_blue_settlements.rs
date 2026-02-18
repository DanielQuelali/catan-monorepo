use fastcore::board_config::board_from_json;
use fastcore::engine::{ArmyState, RoadState};
use fastcore::rng::rng_for_stream;
use fastcore::state::State;
use fastcore::types::{EdgeId, NodeId, PlayerId, Resource, INVALID_TILE, RESOURCE_COUNT};
use fastcore::value_player::{
    apply_value_action, generate_playable_actions, ValueAction, ValueActionKind,
};
use serde_json::Value;
use std::env;
use std::fs::File;
use std::io::BufReader;

const DEFAULT_STATE_PATH: &str = "../state_pre_last_settlement.json";
const DEFAULT_BOARD_PATH: &str = "../board_example.json";
const DEFAULT_SEED: u64 = 99;
const MIN_FOLLOWER_SETTLEMENT_PIPS: u32 = 5;

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

fn truncate_before_color2(
    actions: &[(String, String, Value)],
    color: &str,
) -> Vec<(String, String, Value)> {
    if actions.len() < 2 {
        panic!("not enough actions to derive {}2 base state", color);
    }
    for idx in (0..actions.len() - 1).rev() {
        let (color_a, action_a, _) = &actions[idx];
        let (color_b, action_b, _) = &actions[idx + 1];
        if color_a == color
            && action_a == "BUILD_SETTLEMENT"
            && color_b == color
            && action_b == "BUILD_ROAD"
        {
            return actions[..idx].to_vec();
        }
    }
    panic!(
        "could not find {} settlement+road to trim for {}2 analysis",
        color, color
    );
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

fn pip_count(number: u8) -> u8 {
    match number {
        2 | 12 => 1,
        3 | 11 => 2,
        4 | 10 => 3,
        5 | 9 => 4,
        6 | 8 => 5,
        _ => 0,
    }
}

fn python_resource_index(resource: Resource) -> usize {
    match resource {
        Resource::Brick => 0,
        Resource::Lumber => 1,
        Resource::Wool => 2,
        Resource::Grain => 3,
        Resource::Ore => 4,
    }
}

fn settlement_pips(board: &fastcore::board::Board, node: NodeId) -> [u32; RESOURCE_COUNT] {
    let mut pips = [0u32; RESOURCE_COUNT];
    for tile in board.node_tiles[node as usize] {
        if tile == INVALID_TILE {
            continue;
        }
        let resource = match board.tile_resources[tile as usize] {
            Some(resource) => resource,
            None => continue,
        };
        let number = match board.tile_numbers[tile as usize] {
            Some(number) => number,
            None => continue,
        };
        let pip = pip_count(number) as u32;
        if pip == 0 {
            continue;
        }
        let idx = python_resource_index(resource);
        pips[idx] += pip;
    }
    pips
}

fn settlement_total_pips(board: &fastcore::board::Board, node: NodeId) -> u32 {
    settlement_pips(board, node).iter().sum()
}

fn parse_args() -> (String, String, u64, NodeId, (NodeId, NodeId)) {
    let mut state_path = DEFAULT_STATE_PATH.to_string();
    let mut board_path = DEFAULT_BOARD_PATH.to_string();
    let mut seed = DEFAULT_SEED;
    let mut orange_settlement: Option<NodeId> = None;
    let mut orange_road: Option<(NodeId, NodeId)> = None;

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
                    seed = value.parse().unwrap_or(DEFAULT_SEED);
                }
            }
            "--orange-settlement" => {
                if let Some(value) = args.next() {
                    orange_settlement = value.parse::<NodeId>().ok();
                }
            }
            "--orange-road" => {
                if let Some(value) = args.next() {
                    let parts: Vec<&str> = value.split('-').collect();
                    if parts.len() == 2 {
                        let a = parts[0].parse::<NodeId>().ok();
                        let b = parts[1].parse::<NodeId>().ok();
                        if let (Some(a), Some(b)) = (a, b) {
                            orange_road = Some((a, b));
                        }
                    }
                }
            }
            other => {
                eprintln!("Unknown arg: {other}");
                std::process::exit(2);
            }
        }
    }

    let orange_settlement = orange_settlement.expect("missing --orange-settlement <node>");
    let orange_road = orange_road.expect("missing --orange-road <a-b>");

    (state_path, board_path, seed, orange_settlement, orange_road)
}

fn main() {
    let (state_path, board_path, seed, orange_settlement, orange_road) = parse_args();
    let (actions, colors) = load_actions(&state_path);
    let actions = truncate_before_color2(&actions, "ORANGE");
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

    let orange_player = color_to_player(&colors, "ORANGE");
    let blue_player = color_to_player(&colors, "BLUE");
    let orange_settle = ValueAction {
        player: orange_player,
        kind: ValueActionKind::BuildSettlement(orange_settlement),
    };
    apply_value_action(
        &board,
        &mut state,
        &mut road_state,
        &mut army_state,
        &orange_settle,
        &mut rng,
    );

    let edge = edge_id_for_nodes(&board, orange_road.0, orange_road.1)
        .expect("edge id not found for ORANGE road");
    let orange_road_action = ValueAction {
        player: orange_player,
        kind: ValueActionKind::BuildRoad(edge),
    };
    apply_value_action(
        &board,
        &mut state,
        &mut road_state,
        &mut army_state,
        &orange_road_action,
        &mut rng,
    );

    let mut settlement_nodes: Vec<NodeId> = generate_playable_actions(&board, &state, blue_player)
        .into_iter()
        .filter_map(|action| match action.kind {
            ValueActionKind::BuildSettlement(node) => Some(node),
            _ => None,
        })
        .filter(|node| settlement_total_pips(&board, *node) >= MIN_FOLLOWER_SETTLEMENT_PIPS)
        .collect();
    settlement_nodes.sort_unstable();

    println!(
        "Blue settlement candidates (pips >= {}):",
        MIN_FOLLOWER_SETTLEMENT_PIPS
    );
    for node in settlement_nodes {
        let pips = settlement_total_pips(&board, node);
        println!("{node} (pips {pips})");
    }
}
