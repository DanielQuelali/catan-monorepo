use crate::board::Board;
use crate::state::State;
use crate::types::{
    BuildingLevel, EdgeId, NodeId, PlayerId, PortType, Resource, TileId, EDGE_COUNT, INVALID_EDGE,
    INVALID_NODE, NODE_COUNT, NO_PLAYER, RESOURCE_COUNT, TILE_COUNT,
};

const ROAD_COST: [u8; RESOURCE_COUNT] = [1, 1, 0, 0, 0];
const SETTLEMENT_COST: [u8; RESOURCE_COUNT] = [1, 1, 0, 1, 1];
const CITY_COST: [u8; RESOURCE_COUNT] = [0, 0, 3, 2, 0];
const DEV_CARD_COST: [u8; RESOURCE_COUNT] = [0, 0, 1, 1, 1];
const MAX_ROADS: u8 = 15;
const MAX_SETTLEMENTS: u8 = 5;
const MAX_CITIES: u8 = 4;

fn has_resources(state: &State, player: PlayerId, cost: &[u8; RESOURCE_COUNT]) -> bool {
    let hand = &state.player_resources[player as usize];
    hand.iter()
        .zip(cost.iter())
        .all(|(have, need)| *have >= *need)
}

fn node_adjacent_nodes(board: &Board, node: NodeId) -> [NodeId; 3] {
    let mut out = [INVALID_NODE; 3];
    let mut idx = 0;
    for edge in board.node_edges[node as usize] {
        if edge == INVALID_EDGE {
            continue;
        }
        let nodes = board.edge_nodes[edge as usize];
        let other = if nodes[0] == node { nodes[1] } else { nodes[0] };
        out[idx] = other;
        idx += 1;
        if idx == out.len() {
            break;
        }
    }
    out
}

fn node_has_enemy_building(state: &State, node: NodeId, player: PlayerId) -> bool {
    let owner = state.node_owner[node as usize];
    owner != NO_PLAYER && owner != player
}

fn node_has_friendly_road(board: &Board, state: &State, node: NodeId, player: PlayerId) -> bool {
    for edge in board.node_edges[node as usize] {
        if edge == INVALID_EDGE {
            continue;
        }
        if state.edge_owner[edge as usize] == player {
            return true;
        }
    }
    false
}

fn node_in_component(state: &State, player: PlayerId, node: NodeId) -> bool {
    state.road_components[player as usize].contains_node(node)
}

fn node_is_adjacent_occupied(board: &Board, state: &State, node: NodeId) -> bool {
    for neighbor in node_adjacent_nodes(board, node) {
        if neighbor == INVALID_NODE {
            continue;
        }
        if state.node_owner[neighbor as usize] != NO_PLAYER {
            return true;
        }
    }
    false
}

pub fn is_legal_build_road(board: &Board, state: &State, player: PlayerId, edge: EdgeId) -> bool {
    if player_road_count(state, player) >= MAX_ROADS {
        return false;
    }
    if edge as usize >= EDGE_COUNT {
        return false;
    }
    if state.edge_owner[edge as usize] != NO_PLAYER {
        return false;
    }
    if !has_resources(state, player, &ROAD_COST) {
        return false;
    }

    let nodes = board.edge_nodes[edge as usize];
    nodes
        .iter()
        .any(|node| node_in_component(state, player, *node))
}

pub fn is_legal_build_settlement(
    board: &Board,
    state: &State,
    player: PlayerId,
    node: NodeId,
) -> bool {
    if player_settlement_count(state, player) >= MAX_SETTLEMENTS {
        return false;
    }
    if node as usize >= NODE_COUNT {
        return false;
    }
    if state.node_owner[node as usize] != NO_PLAYER {
        return false;
    }
    if !has_resources(state, player, &SETTLEMENT_COST) {
        return false;
    }
    if node_is_adjacent_occupied(board, state, node) {
        return false;
    }
    node_has_friendly_road(board, state, node, player)
}

pub fn is_legal_build_city(_board: &Board, state: &State, player: PlayerId, node: NodeId) -> bool {
    if player_city_count(state, player) >= MAX_CITIES {
        return false;
    }
    if node as usize >= NODE_COUNT {
        return false;
    }
    if !has_resources(state, player, &CITY_COST) {
        return false;
    }
    if state.node_owner[node as usize] != player {
        return false;
    }
    state.node_level[node as usize] == BuildingLevel::Settlement
}

pub fn is_legal_move_robber(board: &Board, state: &State, tile: TileId) -> bool {
    if tile as usize >= TILE_COUNT {
        return false;
    }
    if tile == state.robber_tile {
        return false;
    }
    let _ = board;
    true
}

pub fn is_legal_maritime_trade(
    board: &Board,
    state: &State,
    player: PlayerId,
    offer: Resource,
    ask: Resource,
) -> bool {
    if offer == ask {
        return false;
    }
    let mut rate = 4u8;
    let mut has_three_to_one = false;
    let mut has_two_to_one = false;

    for (node, port) in board.node_ports.iter().enumerate() {
        if *port == PortType::None {
            continue;
        }
        if state.node_owner[node] != player {
            continue;
        }
        match port {
            PortType::ThreeToOne => has_three_to_one = true,
            PortType::Brick => has_two_to_one |= offer == Resource::Brick,
            PortType::Lumber => has_two_to_one |= offer == Resource::Lumber,
            PortType::Ore => has_two_to_one |= offer == Resource::Ore,
            PortType::Grain => has_two_to_one |= offer == Resource::Grain,
            PortType::Wool => has_two_to_one |= offer == Resource::Wool,
            PortType::None => {}
        }
    }

    if has_two_to_one {
        rate = 2;
    } else if has_three_to_one {
        rate = 3;
    }

    let hand = &state.player_resources[player as usize];
    if hand[offer.as_index()] < rate {
        return false;
    }
    state.bank_resources[ask.as_index()] > 0
}

pub(crate) fn is_legal_initial_settlement(board: &Board, state: &State, node: NodeId) -> bool {
    if node as usize >= NODE_COUNT {
        return false;
    }
    if state.node_owner[node as usize] != NO_PLAYER {
        return false;
    }
    if node_is_adjacent_occupied(board, state, node) {
        return false;
    }
    true
}

pub(crate) fn is_legal_initial_road(
    board: &Board,
    state: &State,
    player: PlayerId,
    edge: EdgeId,
    anchor: NodeId,
) -> bool {
    if edge as usize >= EDGE_COUNT {
        return false;
    }
    if state.edge_owner[edge as usize] != NO_PLAYER {
        return false;
    }
    let nodes = board.edge_nodes[edge as usize];
    if nodes[0] != anchor && nodes[1] != anchor {
        return false;
    }
    !node_has_enemy_building(state, anchor, player)
}

pub(crate) fn road_cost() -> &'static [u8; RESOURCE_COUNT] {
    &ROAD_COST
}

pub(crate) fn settlement_cost() -> &'static [u8; RESOURCE_COUNT] {
    &SETTLEMENT_COST
}

pub(crate) fn city_cost() -> &'static [u8; RESOURCE_COUNT] {
    &CITY_COST
}

pub(crate) fn dev_card_cost() -> &'static [u8; RESOURCE_COUNT] {
    &DEV_CARD_COST
}

fn player_road_count(state: &State, player: PlayerId) -> u8 {
    state
        .edge_owner
        .iter()
        .filter(|owner| **owner == player)
        .count() as u8
}

fn player_settlement_count(state: &State, player: PlayerId) -> u8 {
    state
        .node_owner
        .iter()
        .zip(state.node_level.iter())
        .filter(|(owner, level)| **owner == player && **level == BuildingLevel::Settlement)
        .count() as u8
}

fn player_city_count(state: &State, player: PlayerId) -> u8 {
    state
        .node_owner
        .iter()
        .zip(state.node_level.iter())
        .filter(|(owner, level)| **owner == player && **level == BuildingLevel::City)
        .count() as u8
}
