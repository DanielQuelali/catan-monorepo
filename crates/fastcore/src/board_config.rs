use crate::board::board_from_layout;
use crate::types::{NodeId, PortType, Resource, TileId, NODE_COUNT, TILE_COUNT};
use serde::Deserialize;
use std::fs::File;
use std::io::BufReader;

const PORT_NODE_PAIRS: [(NodeId, NodeId); 9] = [
    (25, 26),
    (28, 29),
    (32, 33),
    (35, 36),
    (38, 39),
    (40, 44),
    (45, 47),
    (48, 49),
    (52, 53),
];

#[derive(Deserialize)]
struct RawBoardConfig {
    tile_resources: Vec<Option<String>>,
    port_resources: Vec<Option<String>>,
    numbers: Vec<u8>,
}

pub fn board_from_json(path: &str) -> Result<crate::board::Board, String> {
    let file = File::open(path).map_err(|err| format!("failed to open {path}: {err}"))?;
    let raw: RawBoardConfig =
        serde_json::from_reader(BufReader::new(file)).map_err(|err| err.to_string())?;

    if raw.tile_resources.len() != TILE_COUNT {
        return Err(format!(
            "tile_resources expected {TILE_COUNT}, got {}",
            raw.tile_resources.len()
        ));
    }
    if raw.port_resources.len() != PORT_NODE_PAIRS.len() {
        return Err(format!(
            "port_resources expected {}, got {}",
            PORT_NODE_PAIRS.len(),
            raw.port_resources.len()
        ));
    }

    let mut tile_resources = [None; TILE_COUNT];
    for (idx, value) in raw.tile_resources.iter().enumerate() {
        tile_resources[idx] = match value {
            Some(name) => Some(parse_resource(name)?),
            None => None,
        };
    }

    let mut tile_numbers = [None; TILE_COUNT];
    let mut number_iter = raw.numbers.iter();
    let mut desert_tile: TileId = 0;
    for (idx, resource) in tile_resources.iter().enumerate() {
        if resource.is_none() {
            desert_tile = idx as TileId;
            tile_numbers[idx] = None;
            continue;
        }
        let number = number_iter
            .next()
            .ok_or_else(|| "numbers list too short".to_string())?;
        tile_numbers[idx] = Some(*number);
    }
    if number_iter.next().is_some() {
        return Err("numbers list too long".to_string());
    }

    let mut node_ports = [PortType::None; NODE_COUNT];
    for (idx, value) in raw.port_resources.iter().enumerate() {
        let port_type = match value {
            Some(name) => port_type_from_resource(parse_resource(name)?),
            None => PortType::ThreeToOne,
        };
        let (a, b) = PORT_NODE_PAIRS[idx];
        node_ports[a as usize] = port_type;
        node_ports[b as usize] = port_type;
    }

    Ok(board_from_layout(
        tile_resources,
        tile_numbers,
        node_ports,
        desert_tile,
    ))
}

fn parse_resource(value: &str) -> Result<Resource, String> {
    match value.trim().to_ascii_uppercase().as_str() {
        "WOOD" | "LUMBER" => Ok(Resource::Lumber),
        "BRICK" => Ok(Resource::Brick),
        "SHEEP" | "WOOL" => Ok(Resource::Wool),
        "WHEAT" | "GRAIN" => Ok(Resource::Grain),
        "ORE" => Ok(Resource::Ore),
        other => Err(format!("unknown resource: {other}")),
    }
}

fn port_type_from_resource(resource: Resource) -> PortType {
    match resource {
        Resource::Brick => PortType::Brick,
        Resource::Lumber => PortType::Lumber,
        Resource::Ore => PortType::Ore,
        Resource::Grain => PortType::Grain,
        Resource::Wool => PortType::Wool,
    }
}
