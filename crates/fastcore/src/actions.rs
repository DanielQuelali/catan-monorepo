use crate::types::{EdgeId, NodeId, TileId};

const KIND_SHIFT: u32 = 27;
const PAYLOAD_MASK: u32 = (1u32 << KIND_SHIFT) - 1;

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ActionKind {
    Invalid = 0,
    BuildRoad = 1,
    BuildSettlement = 2,
    BuildCity = 3,
    MoveRobber = 4,
    MaritimeTrade = 5,
    DomesticTrade = 6,
    RollDice = 7,
    EndTurn = 8,
    BuyDevCard = 9,
    PlayDevCard = 10,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ActionCode(pub u32);

impl ActionCode {
    pub fn new(kind: ActionKind, payload: u32) -> Self {
        ActionCode(((kind as u32) << KIND_SHIFT) | (payload & PAYLOAD_MASK))
    }

    pub fn kind(self) -> ActionKind {
        match (self.0 >> KIND_SHIFT) as u8 {
            1 => ActionKind::BuildRoad,
            2 => ActionKind::BuildSettlement,
            3 => ActionKind::BuildCity,
            4 => ActionKind::MoveRobber,
            5 => ActionKind::MaritimeTrade,
            6 => ActionKind::DomesticTrade,
            7 => ActionKind::RollDice,
            8 => ActionKind::EndTurn,
            9 => ActionKind::BuyDevCard,
            10 => ActionKind::PlayDevCard,
            _ => ActionKind::Invalid,
        }
    }

    pub fn payload(self) -> u32 {
        self.0 & PAYLOAD_MASK
    }

    pub fn build_road(edge: EdgeId) -> Self {
        Self::new(ActionKind::BuildRoad, edge as u32)
    }

    pub fn build_settlement(node: NodeId) -> Self {
        Self::new(ActionKind::BuildSettlement, node as u32)
    }

    pub fn build_city(node: NodeId) -> Self {
        Self::new(ActionKind::BuildCity, node as u32)
    }

    pub fn move_robber(tile: TileId) -> Self {
        Self::new(ActionKind::MoveRobber, tile as u32)
    }
}

impl From<u32> for ActionCode {
    fn from(value: u32) -> Self {
        ActionCode(value)
    }
}

impl From<ActionCode> for u32 {
    fn from(value: ActionCode) -> Self {
        value.0
    }
}
