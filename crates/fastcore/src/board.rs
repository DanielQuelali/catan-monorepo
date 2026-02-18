use crate::board_data::{
    DESERT_TILE, EDGE_NODES, NODE_EDGES, NODE_PORTS, NODE_TILES, TILE_NODES, TILE_NUMBERS,
    TILE_RESOURCES,
};
use crate::types::{
    EdgeId, NodeId, PortType, Resource, TileId, EDGE_COUNT, NODE_COUNT, TILE_COUNT,
};

pub const NODE_DEGREE: usize = 3;
pub const NODE_TILE_DEGREE: usize = 3;
pub const TILE_DEGREE: usize = 6;

#[derive(Copy, Clone, Debug)]
pub struct Board {
    pub node_edges: [[EdgeId; NODE_DEGREE]; NODE_COUNT],
    pub edge_nodes: [[NodeId; 2]; EDGE_COUNT],
    pub node_tiles: [[TileId; NODE_TILE_DEGREE]; NODE_COUNT],
    pub tile_nodes: [[NodeId; TILE_DEGREE]; TILE_COUNT],
    pub node_ports: [PortType; NODE_COUNT],
    pub tile_resources: [Option<Resource>; TILE_COUNT],
    pub tile_numbers: [Option<u8>; TILE_COUNT],
    pub desert_tile: TileId,
}

impl Board {
    pub fn standard() -> &'static Board {
        &STANDARD_BOARD
    }
}

pub fn board_from_layout(
    tile_resources: [Option<Resource>; TILE_COUNT],
    tile_numbers: [Option<u8>; TILE_COUNT],
    node_ports: [PortType; NODE_COUNT],
    desert_tile: TileId,
) -> Board {
    Board {
        node_edges: NODE_EDGES,
        edge_nodes: EDGE_NODES,
        node_tiles: NODE_TILES,
        tile_nodes: TILE_NODES,
        node_ports,
        tile_resources,
        tile_numbers,
        desert_tile,
    }
}

pub fn tile_coords(tile: TileId) -> (i8, i8, i8) {
    crate::board_data::TILE_COORDS
        .get(tile as usize)
        .copied()
        .unwrap_or((0, 0, 0))
}

pub const STANDARD_BOARD: Board = Board {
    node_edges: NODE_EDGES,
    edge_nodes: EDGE_NODES,
    node_tiles: NODE_TILES,
    tile_nodes: TILE_NODES,
    node_ports: NODE_PORTS,
    tile_resources: TILE_RESOURCES,
    tile_numbers: TILE_NUMBERS,
    desert_tile: DESERT_TILE,
};
