use crate::types::{
    BuildingLevel, EdgeId, NodeId, PlayerId, Resource, TileId, TurnPhase, NO_PLAYER,
};

const MAX_ROAD_DELTAS: usize = 32;
const MAX_BUILDING_DELTAS: usize = 32;
const MAX_RESOURCE_DELTAS: usize = 256;
const MAX_BANK_DELTAS: usize = 256;

#[derive(Copy, Clone, Debug)]
pub(crate) struct RoadDelta {
    pub(crate) edge: EdgeId,
    pub(crate) prev_owner: PlayerId,
}

impl Default for RoadDelta {
    fn default() -> Self {
        Self {
            edge: 0,
            prev_owner: NO_PLAYER,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct BuildingDelta {
    pub(crate) node: NodeId,
    pub(crate) prev_owner: PlayerId,
    pub(crate) prev_level: BuildingLevel,
}

impl Default for BuildingDelta {
    fn default() -> Self {
        Self {
            node: 0,
            prev_owner: NO_PLAYER,
            prev_level: BuildingLevel::Empty,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ResourceDelta {
    pub(crate) player: PlayerId,
    pub(crate) resource: Resource,
    pub(crate) prev: u8,
}

impl Default for ResourceDelta {
    fn default() -> Self {
        Self {
            player: 0,
            resource: Resource::Brick,
            prev: 0,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct BankDelta {
    pub(crate) resource: Resource,
    pub(crate) prev: u8,
}

impl Default for BankDelta {
    fn default() -> Self {
        Self {
            resource: Resource::Brick,
            prev: 0,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct TurnDelta {
    pub(crate) player: PlayerId,
    pub(crate) phase: TurnPhase,
}

#[derive(Clone, Debug)]
pub struct Delta {
    road_deltas: [RoadDelta; MAX_ROAD_DELTAS],
    road_len: usize,
    building_deltas: [BuildingDelta; MAX_BUILDING_DELTAS],
    building_len: usize,
    resource_deltas: [ResourceDelta; MAX_RESOURCE_DELTAS],
    resource_len: usize,
    bank_deltas: [BankDelta; MAX_BANK_DELTAS],
    bank_len: usize,
    turn: Option<TurnDelta>,
    robber_tile: Option<TileId>,
}

impl Default for Delta {
    fn default() -> Self {
        Self {
            road_deltas: [RoadDelta::default(); MAX_ROAD_DELTAS],
            road_len: 0,
            building_deltas: [BuildingDelta::default(); MAX_BUILDING_DELTAS],
            building_len: 0,
            resource_deltas: [ResourceDelta::default(); MAX_RESOURCE_DELTAS],
            resource_len: 0,
            bank_deltas: [BankDelta::default(); MAX_BANK_DELTAS],
            bank_len: 0,
            turn: None,
            robber_tile: None,
        }
    }
}

impl Delta {
    pub fn reset(&mut self) {
        self.road_len = 0;
        self.building_len = 0;
        self.resource_len = 0;
        self.bank_len = 0;
        self.turn = None;
        self.robber_tile = None;
    }

    pub fn record_road(&mut self, edge: EdgeId, prev_owner: PlayerId) {
        debug_assert!(self.road_len < MAX_ROAD_DELTAS);
        if self.road_len >= MAX_ROAD_DELTAS {
            return;
        }
        self.road_deltas[self.road_len] = RoadDelta { edge, prev_owner };
        self.road_len += 1;
    }

    pub fn record_building(
        &mut self,
        node: NodeId,
        prev_owner: PlayerId,
        prev_level: BuildingLevel,
    ) {
        debug_assert!(self.building_len < MAX_BUILDING_DELTAS);
        if self.building_len >= MAX_BUILDING_DELTAS {
            return;
        }
        self.building_deltas[self.building_len] = BuildingDelta {
            node,
            prev_owner,
            prev_level,
        };
        self.building_len += 1;
    }

    pub fn record_resource(&mut self, player: PlayerId, resource: Resource, prev: u8) {
        debug_assert!(self.resource_len < MAX_RESOURCE_DELTAS);
        if self.resource_len >= MAX_RESOURCE_DELTAS {
            return;
        }
        self.resource_deltas[self.resource_len] = ResourceDelta {
            player,
            resource,
            prev,
        };
        self.resource_len += 1;
    }

    pub fn record_bank(&mut self, resource: Resource, prev: u8) {
        debug_assert!(self.bank_len < MAX_BANK_DELTAS);
        if self.bank_len >= MAX_BANK_DELTAS {
            return;
        }
        self.bank_deltas[self.bank_len] = BankDelta { resource, prev };
        self.bank_len += 1;
    }

    pub fn record_turn(&mut self, player: PlayerId, phase: TurnPhase) {
        self.turn = Some(TurnDelta { player, phase });
    }

    pub fn record_robber(&mut self, tile: TileId) {
        self.robber_tile = Some(tile);
    }

    pub(crate) fn road_deltas(&self) -> (&[RoadDelta], usize) {
        (&self.road_deltas, self.road_len)
    }

    pub(crate) fn building_deltas(&self) -> (&[BuildingDelta], usize) {
        (&self.building_deltas, self.building_len)
    }

    pub(crate) fn resource_deltas(&self) -> (&[ResourceDelta], usize) {
        (&self.resource_deltas, self.resource_len)
    }

    pub(crate) fn bank_deltas(&self) -> (&[BankDelta], usize) {
        (&self.bank_deltas, self.bank_len)
    }

    pub(crate) fn turn(&self) -> Option<TurnDelta> {
        self.turn
    }

    pub(crate) fn robber_tile(&self) -> Option<TileId> {
        self.robber_tile
    }
}
