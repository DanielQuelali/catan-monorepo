use fastcore::board_config::board_from_json;
use fastcore::engine::{ArmyState, RoadState};
use fastcore::rng::rng_for_stream;
use fastcore::state::State;
use fastcore::types::{BuildingLevel, DevCard, EdgeId, NodeId, PlayerId, PLAYER_COUNT};
use fastcore::value_player::{
    apply_value_action_kernel, FastValueFunctionPlayer, ValueAction, ValueActionKind,
};
use serde_json::Value;
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::time::Instant;

const DEFAULT_STATE_PATH: &str = "../state_pre_last_settlement.json";
const DEFAULT_SIM_COUNT: u32 = 20;
const DEFAULT_SEED_START: u64 = 1;
const DEFAULT_MAX_TURNS: u32 = 2000;
const DEFAULT_BOARD_PATH: &str = "../board_example.json";

fn parse_args() -> (String, String, u32, u64, u32) {
    let mut state_path = DEFAULT_STATE_PATH.to_string();
    let mut board_path = DEFAULT_BOARD_PATH.to_string();
    let mut sims = DEFAULT_SIM_COUNT;
    let mut seed_start = DEFAULT_SEED_START;
    let mut max_turns = DEFAULT_MAX_TURNS;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => {
                if let Some(value) = args.next() {
                    state_path = value;
                }
            }
            "--sims" => {
                if let Some(value) = args.next() {
                    sims = value.parse().unwrap_or(DEFAULT_SIM_COUNT);
                }
            }
            "--board" => {
                if let Some(value) = args.next() {
                    board_path = value;
                }
            }
            "--seed" => {
                if let Some(value) = args.next() {
                    seed_start = value.parse().unwrap_or(DEFAULT_SEED_START);
                }
            }
            "--max-turns" => {
                if let Some(value) = args.next() {
                    max_turns = value.parse().unwrap_or(DEFAULT_MAX_TURNS);
                }
            }
            "-h" | "--help" => {
                eprintln!(
                    "Usage: bench_value_state [--state <path>] [--board <path>] [--sims <count>] [--seed <start>] [--max-turns <num>]"
                );
                std::process::exit(0);
            }
            _ => {
                eprintln!("Unknown arg: {arg}");
                std::process::exit(2);
            }
        }
    }

    (state_path, board_path, sims, seed_start, max_turns)
}

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

fn apply_initial_actions(
    board: &fastcore::board::Board,
    actions: &[(String, String, Value)],
    colors: &[String],
    rng: &mut impl rand_core::RngCore,
) -> (State, RoadState, ArmyState) {
    let mut state = State::new_with_rng_and_board(rng, board);
    let mut road_state = RoadState::empty();
    let mut army_state = ArmyState::empty();

    for (color, action_type, payload) in actions {
        let player = color_to_player(colors, color);
        if state.active_player != player {
            state.active_player = player;
            state.turn_player = player;
        }
        let value_action = match action_type.as_str() {
            "BUILD_SETTLEMENT" => {
                let node = payload
                    .as_u64()
                    .expect("expected node id for BUILD_SETTLEMENT")
                    as NodeId;
                ValueAction {
                    player,
                    kind: ValueActionKind::BuildSettlement(node),
                }
            }
            "BUILD_ROAD" => {
                let pair = payload
                    .as_array()
                    .expect("expected node pair for BUILD_ROAD");
                let a = pair[0].as_u64().expect("expected edge node id") as NodeId;
                let b = pair[1].as_u64().expect("expected edge node id") as NodeId;
                let edge =
                    edge_id_for_nodes(board, a, b).expect("edge id not found for BUILD_ROAD");
                ValueAction {
                    player,
                    kind: ValueActionKind::BuildRoad(edge),
                }
            }
            other => {
                panic!("unsupported action type in initial state: {other}");
            }
        };
        apply_value_action_kernel(
            board,
            &mut state,
            &mut road_state,
            &mut army_state,
            &value_action,
            rng,
        );
    }

    (state, road_state, army_state)
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

fn check_winner(state: &State, road_state: &RoadState, army_state: &ArmyState) -> Option<PlayerId> {
    let points = player_points(state, road_state, army_state);
    for (player, score) in points.iter().enumerate() {
        if *score >= 10 {
            return Some(player as u8);
        }
    }
    None
}

#[inline]
fn action_may_change_points(kind: &ValueActionKind) -> bool {
    matches!(
        kind,
        ValueActionKind::BuildSettlement(_)
            | ValueActionKind::BuildRoad(_)
            | ValueActionKind::BuildCity(_)
            | ValueActionKind::PlayKnight
            | ValueActionKind::BuyDevCard
    )
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

fn simulate_from_state(
    board: &fastcore::board::Board,
    mut state: State,
    mut road_state: RoadState,
    mut army_state: ArmyState,
    rng: &mut impl rand_core::RngCore,
    max_turns: u32,
) -> (PlayerId, u32) {
    let base_turns = state.num_turns;
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
        let action = players[player].decide(board, &state, &road_state, &army_state, rng);
        apply_value_action_kernel(
            board,
            &mut state,
            &mut road_state,
            &mut army_state,
            &action,
            rng,
        );

        if action_may_change_points(&action.kind) {
            if check_winner(&state, &road_state, &army_state).is_some() {
                break;
            }
        }
    }

    let winner = check_winner(&state, &road_state, &army_state)
        .unwrap_or_else(|| select_winner(&state, &road_state, &army_state));
    let turns = state.num_turns.saturating_sub(base_turns);
    (winner, turns)
}

fn main() {
    let (state_path, board_path, sims, seed_start, max_turns) = parse_args();
    let (actions, colors) = load_actions(&state_path);
    let board = board_from_json(&board_path).expect("failed to load board json");

    let start = Instant::now();
    let mut wins = [0u32; PLAYER_COUNT];
    let mut total_turns = 0u64;

    for offset in 0..sims {
        let seed = seed_start + offset as u64;
        let mut rng = rng_for_stream(seed, 0);
        let (state, road_state, army_state) =
            apply_initial_actions(&board, &actions, &colors, &mut rng);
        let (winner, turns) =
            simulate_from_state(&board, state, road_state, army_state, &mut rng, max_turns);
        wins[winner as usize] += 1;
        total_turns += turns as u64;
    }

    let elapsed = start.elapsed().as_secs_f64();
    let games = sims as f64;
    let avg_ms = if games > 0.0 {
        (elapsed * 1000.0) / games
    } else {
        0.0
    };
    let gps = if elapsed > 0.0 { games / elapsed } else { 0.0 };

    println!(
        "rust: games={} turns={} wins={},{},{},{} time_s={:.3} avg_ms={:.3} games_per_sec={:.2}",
        sims, total_turns, wins[0], wins[1], wins[2], wins[3], elapsed, avg_ms, gps
    );
}
