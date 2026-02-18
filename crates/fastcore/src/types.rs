pub const PLAYER_COUNT: usize = 4;
pub const RESOURCE_COUNT: usize = 5;
pub const DEV_CARD_COUNT: usize = 5;
pub const NODE_COUNT: usize = 54;
pub const EDGE_COUNT: usize = 72;
pub const TILE_COUNT: usize = 19;

pub type PlayerId = u8;
pub type NodeId = u8;
pub type EdgeId = u8;
pub type TileId = u8;

pub const NO_PLAYER: PlayerId = u8::MAX;
pub const INVALID_NODE: NodeId = u8::MAX;
pub const INVALID_EDGE: EdgeId = u8::MAX;
pub const INVALID_TILE: TileId = u8::MAX;

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Resource {
    Brick = 0,
    Lumber = 1,
    Ore = 2,
    Grain = 3,
    Wool = 4,
}

impl Resource {
    pub const ALL: [Resource; RESOURCE_COUNT] = [
        Resource::Lumber,
        Resource::Brick,
        Resource::Wool,
        Resource::Grain,
        Resource::Ore,
    ];

    pub fn as_index(self) -> usize {
        self as usize
    }

    pub fn from_index(index: usize) -> Option<Resource> {
        match index {
            0 => Some(Resource::Brick),
            1 => Some(Resource::Lumber),
            2 => Some(Resource::Ore),
            3 => Some(Resource::Grain),
            4 => Some(Resource::Wool),
            _ => None,
        }
    }
}

pub const PYTHON_RESOURCE_ORDER: [Resource; RESOURCE_COUNT] = [
    Resource::Lumber,
    Resource::Brick,
    Resource::Wool,
    Resource::Grain,
    Resource::Ore,
];

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BuildingLevel {
    Empty = 0,
    Settlement = 1,
    City = 2,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TurnPhase {
    Setup = 0,
    Roll = 1,
    Main = 2,
    Discard = 3,
    Robber = 4,
    Trade = 5,
    End = 6,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PortType {
    None = 0,
    ThreeToOne = 1,
    Brick = 2,
    Lumber = 3,
    Ore = 4,
    Grain = 5,
    Wool = 6,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DevCard {
    Knight = 0,
    YearOfPlenty = 1,
    Monopoly = 2,
    RoadBuilding = 3,
    VictoryPoint = 4,
}

impl DevCard {
    pub const ALL: [DevCard; DEV_CARD_COUNT] = [
        DevCard::Knight,
        DevCard::YearOfPlenty,
        DevCard::Monopoly,
        DevCard::RoadBuilding,
        DevCard::VictoryPoint,
    ];

    pub fn as_index(self) -> usize {
        self as usize
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ActionPrompt {
    BuildInitialSettlement = 0,
    BuildInitialRoad = 1,
    PlayTurn = 2,
    Discard = 3,
    MoveRobber = 4,
    DecideTrade = 5,
    DecideAcceptees = 6,
}
