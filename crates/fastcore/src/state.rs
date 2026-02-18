use crate::board::Board;
use crate::board::STANDARD_BOARD;
use crate::delta::Delta;
use crate::rng::shuffle_with_rng;
use crate::types::{
    ActionPrompt, BuildingLevel, DevCard, EdgeId, NodeId, PlayerId, Resource, TileId, TurnPhase,
    DEV_CARD_COUNT, EDGE_COUNT, INVALID_NODE, NODE_COUNT, NO_PLAYER, PLAYER_COUNT, RESOURCE_COUNT,
};
use rand_core::RngCore;

const DEFAULT_BANK_RESOURCE: u8 = 19;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub turn_player: PlayerId,
    pub active_player: PlayerId,
    pub turn_phase: TurnPhase,
    pub robber_tile: TileId,
    pub bank_resources: [u8; RESOURCE_COUNT],
    pub player_resources: [[u8; RESOURCE_COUNT]; PLAYER_COUNT],
    pub dev_cards_in_hand: [[u8; DEV_CARD_COUNT]; PLAYER_COUNT],
    pub dev_cards_played: [[u8; DEV_CARD_COUNT]; PLAYER_COUNT],
    pub dev_owned_at_start: [[bool; DEV_CARD_COUNT]; PLAYER_COUNT],
    pub has_rolled: [bool; PLAYER_COUNT],
    pub has_played_dev: [bool; PLAYER_COUNT],
    pub edge_owner: [PlayerId; EDGE_COUNT],
    pub node_owner: [PlayerId; NODE_COUNT],
    pub node_level: [BuildingLevel; NODE_COUNT],
    pub road_components: [Vec<Vec<NodeId>>; PLAYER_COUNT],
    pub dev_deck: Vec<DevCard>,
    pub current_prompt: ActionPrompt,
    pub is_initial_build_phase: bool,
    pub is_discarding: bool,
    pub is_moving_robber: bool,
    pub is_road_building: bool,
    pub free_roads_available: u8,
    pub is_resolving_trade: bool,
    pub current_trade: [u8; RESOURCE_COUNT * 2],
    pub trade_offering_player: PlayerId,
    pub acceptees: [bool; PLAYER_COUNT],
    pub trade_offered_this_turn: bool,
    pub num_turns: u32,
    pub last_initial_settlement: [NodeId; PLAYER_COUNT],
}

impl State {
    pub fn new() -> Self {
        Self::new_with_dev_deck(build_dev_deck(), STANDARD_BOARD.desert_tile)
    }

    pub fn new_with_rng(rng: &mut impl RngCore) -> Self {
        let mut dev_deck = build_dev_deck();
        shuffle_with_rng(&mut dev_deck, rng);
        Self::new_with_dev_deck(dev_deck, STANDARD_BOARD.desert_tile)
    }

    pub fn new_with_rng_and_board(rng: &mut impl RngCore, board: &Board) -> Self {
        let mut dev_deck = build_dev_deck();
        shuffle_with_rng(&mut dev_deck, rng);
        Self::new_with_dev_deck(dev_deck, board.desert_tile)
    }

    fn new_with_dev_deck(dev_deck: Vec<DevCard>, desert_tile: TileId) -> Self {
        Self {
            turn_player: 0,
            active_player: 0,
            turn_phase: TurnPhase::Setup,
            robber_tile: desert_tile,
            bank_resources: [DEFAULT_BANK_RESOURCE; RESOURCE_COUNT],
            player_resources: [[0; RESOURCE_COUNT]; PLAYER_COUNT],
            dev_cards_in_hand: [[0; DEV_CARD_COUNT]; PLAYER_COUNT],
            dev_cards_played: [[0; DEV_CARD_COUNT]; PLAYER_COUNT],
            dev_owned_at_start: [[false; DEV_CARD_COUNT]; PLAYER_COUNT],
            has_rolled: [false; PLAYER_COUNT],
            has_played_dev: [false; PLAYER_COUNT],
            edge_owner: [NO_PLAYER; EDGE_COUNT],
            node_owner: [NO_PLAYER; NODE_COUNT],
            node_level: [BuildingLevel::Empty; NODE_COUNT],
            road_components: std::array::from_fn(|_| Vec::new()),
            dev_deck,
            current_prompt: ActionPrompt::BuildInitialSettlement,
            is_initial_build_phase: true,
            is_discarding: false,
            is_moving_robber: false,
            is_road_building: false,
            free_roads_available: 0,
            is_resolving_trade: false,
            current_trade: [0; RESOURCE_COUNT * 2],
            trade_offering_player: 0,
            acceptees: [false; PLAYER_COUNT],
            trade_offered_this_turn: false,
            num_turns: 0,
            last_initial_settlement: [INVALID_NODE; PLAYER_COUNT],
        }
    }

    pub fn set_turn(&mut self, player: PlayerId, phase: TurnPhase, delta: &mut Delta) {
        delta.record_turn(self.turn_player, self.turn_phase);
        self.turn_player = player;
        self.turn_phase = phase;
    }

    #[inline]
    pub fn set_turn_kernel(&mut self, player: PlayerId, phase: TurnPhase) {
        self.turn_player = player;
        self.turn_phase = phase;
    }

    pub fn move_robber(&mut self, tile: TileId, delta: &mut Delta) {
        delta.record_robber(self.robber_tile);
        self.robber_tile = tile;
    }

    #[inline]
    pub fn move_robber_kernel(&mut self, tile: TileId) {
        self.robber_tile = tile;
    }

    pub fn set_road_owner(&mut self, edge: EdgeId, owner: PlayerId, delta: &mut Delta) {
        let idx = edge as usize;
        delta.record_road(edge, self.edge_owner[idx]);
        self.edge_owner[idx] = owner;
    }

    #[inline]
    pub fn set_road_owner_kernel(&mut self, edge: EdgeId, owner: PlayerId) {
        self.edge_owner[edge as usize] = owner;
    }

    pub fn set_building(
        &mut self,
        node: NodeId,
        owner: PlayerId,
        level: BuildingLevel,
        delta: &mut Delta,
    ) {
        let idx = node as usize;
        delta.record_building(node, self.node_owner[idx], self.node_level[idx]);
        self.node_owner[idx] = owner;
        self.node_level[idx] = level;
    }

    #[inline]
    pub fn set_building_kernel(&mut self, node: NodeId, owner: PlayerId, level: BuildingLevel) {
        let idx = node as usize;
        self.node_owner[idx] = owner;
        self.node_level[idx] = level;
    }

    pub fn adjust_resource(
        &mut self,
        player: PlayerId,
        resource: Resource,
        amount: i8,
        delta: &mut Delta,
    ) {
        let p_idx = player as usize;
        let r_idx = resource.as_index();
        let prev = self.player_resources[p_idx][r_idx];
        delta.record_resource(player, resource, prev);
        let updated = prev as i16 + amount as i16;
        debug_assert!(updated >= 0 && updated <= u8::MAX as i16);
        self.player_resources[p_idx][r_idx] = updated as u8;
    }

    #[inline]
    pub fn adjust_resource_kernel(&mut self, player: PlayerId, resource: Resource, amount: i8) {
        let p_idx = player as usize;
        let r_idx = resource.as_index();
        let prev = self.player_resources[p_idx][r_idx];
        let updated = prev as i16 + amount as i16;
        debug_assert!(updated >= 0 && updated <= u8::MAX as i16);
        self.player_resources[p_idx][r_idx] = updated as u8;
    }

    pub fn adjust_bank(&mut self, resource: Resource, amount: i8, delta: &mut Delta) {
        let r_idx = resource.as_index();
        let prev = self.bank_resources[r_idx];
        delta.record_bank(resource, prev);
        let updated = prev as i16 + amount as i16;
        debug_assert!(updated >= 0 && updated <= u8::MAX as i16);
        self.bank_resources[r_idx] = updated as u8;
    }

    #[inline]
    pub fn adjust_bank_kernel(&mut self, resource: Resource, amount: i8) {
        let r_idx = resource.as_index();
        let prev = self.bank_resources[r_idx];
        let updated = prev as i16 + amount as i16;
        debug_assert!(updated >= 0 && updated <= u8::MAX as i16);
        self.bank_resources[r_idx] = updated as u8;
    }

    pub fn undo(&mut self, delta: &Delta) {
        let (roads, road_len) = delta.road_deltas();
        for i in (0..road_len).rev() {
            let change = roads[i];
            self.edge_owner[change.edge as usize] = change.prev_owner;
        }

        let (buildings, building_len) = delta.building_deltas();
        for i in (0..building_len).rev() {
            let change = buildings[i];
            let idx = change.node as usize;
            self.node_owner[idx] = change.prev_owner;
            self.node_level[idx] = change.prev_level;
        }

        let (resources, resource_len) = delta.resource_deltas();
        for i in (0..resource_len).rev() {
            let change = resources[i];
            self.player_resources[change.player as usize][change.resource.as_index()] = change.prev;
        }

        let (banks, bank_len) = delta.bank_deltas();
        for i in (0..bank_len).rev() {
            let change = banks[i];
            self.bank_resources[change.resource.as_index()] = change.prev;
        }

        if let Some(turn) = delta.turn() {
            self.turn_player = turn.player;
            self.turn_phase = turn.phase;
        }

        if let Some(tile) = delta.robber_tile() {
            self.robber_tile = tile;
        }
    }
}

fn build_dev_deck() -> Vec<DevCard> {
    let mut dev_deck = Vec::with_capacity(25);
    dev_deck.extend([DevCard::Knight; 14]);
    dev_deck.extend([DevCard::YearOfPlenty; 2]);
    dev_deck.extend([DevCard::RoadBuilding; 2]);
    dev_deck.extend([DevCard::Monopoly; 2]);
    dev_deck.extend([DevCard::VictoryPoint; 5]);
    dev_deck
}
