use crate::board::Board;
use crate::board_data::{EDGE_NODES, TILE_COORDS};
use crate::delta::Delta;
use crate::engine::{
    apply_accept_trade, apply_build_city, apply_build_city_kernel, apply_build_road,
    apply_build_road_kernel, apply_build_settlement, apply_build_settlement_kernel,
    apply_cancel_trade, apply_confirm_trade, apply_confirm_trade_kernel, apply_discard,
    apply_discard_kernel, apply_end_turn, apply_initial_road, apply_initial_road_kernel,
    apply_initial_settlement, apply_initial_settlement_kernel, apply_knight, apply_maritime_trade,
    apply_maritime_trade_kernel, apply_monopoly, apply_monopoly_kernel, apply_move_robber,
    apply_move_robber_kernel, apply_reject_trade, apply_road_building, apply_roll,
    apply_roll_kernel, apply_year_of_plenty, apply_year_of_plenty_kernel, buy_dev_card,
    buy_dev_card_kernel, can_accept_trade, can_buy_dev_card, can_play_dev,
    is_legal_build_road_free, player_has_building_on_tile, player_resource_total, trade_rate,
    ArmyState, RoadState,
};
use crate::rng::{next_u64_mod, roll_die, shuffle_with_rng};
use crate::rules;
use crate::state::{DevDeck, RoadComponents, State};
use crate::types::{
    ActionPrompt, BuildingLevel, DevCard, EdgeId, NodeId, PlayerId, PortType, Resource, TileId,
    TurnPhase, DEV_CARD_COUNT, EDGE_COUNT, INVALID_EDGE, INVALID_NODE, INVALID_TILE, NODE_COUNT,
    NO_PLAYER, PLAYER_COUNT, PYTHON_RESOURCE_ORDER, RESOURCE_COUNT, TILE_COUNT,
};
use rand_core::RngCore;
use std::cmp::Ordering;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::OnceLock;

const RESOURCE_ORDER: [Resource; RESOURCE_COUNT] = [
    Resource::Lumber,
    Resource::Brick,
    Resource::Wool,
    Resource::Grain,
    Resource::Ore,
];

const DICE_PROBAS: [f64; 13] = [
    0.0,
    0.0,
    1.0 / 36.0,
    2.0 / 36.0,
    3.0 / 36.0,
    4.0 / 36.0,
    5.0 / 36.0,
    6.0 / 36.0,
    5.0 / 36.0,
    4.0 / 36.0,
    3.0 / 36.0,
    2.0 / 36.0,
    1.0 / 36.0,
];

const TRANSLATE_VARIETY: f64 = 4.0;
const PROBA_POINT: f64 = 2.778 / 100.0;
const ENDGAME_GAP_WEIGHT: f64 = 50_000_000.0;
#[cfg(feature = "save_first_city_after_first_dev")]
const DEV_HOARDING_OVERRIDE_CARDS: u8 = 8;
static ENDGAME_GAP_ENABLED: AtomicBool = AtomicBool::new(true);

static NODE_PRODUCTION: OnceLock<[[f64; RESOURCE_COUNT]; NODE_COUNT]> = OnceLock::new();

pub fn set_endgame_gap_enabled(enabled: bool) {
    ENDGAME_GAP_ENABLED.store(enabled, AtomicOrdering::Relaxed);
}

pub fn endgame_gap_enabled() -> bool {
    ENDGAME_GAP_ENABLED.load(AtomicOrdering::Relaxed)
}

#[cfg(feature = "save_first_city_after_first_dev")]
fn player_has_city(state: &State, player: PlayerId) -> bool {
    state
        .node_owner
        .iter()
        .zip(state.node_level.iter())
        .any(|(owner, level)| *owner == player && *level == BuildingLevel::City)
}

#[cfg(feature = "save_first_city_after_first_dev")]
fn player_has_any_dev_card(state: &State, player: PlayerId) -> bool {
    let in_hand: u8 = state.dev_cards_in_hand[player as usize].iter().copied().sum();
    let played: u8 = state.dev_cards_played[player as usize].iter().copied().sum();
    in_hand.saturating_add(played) > 0
}

#[cfg(feature = "save_first_city_after_first_dev")]
fn should_save_for_first_city_after_first_dev(state: &State, player: PlayerId) -> bool {
    if !player_has_any_dev_card(state, player) || player_has_city(state, player) {
        return false;
    }
    let cards_in_hand = player_resource_total(state, player);
    cards_in_hand < DEV_HOARDING_OVERRIDE_CARDS
}

#[cfg(feature = "save_first_city_after_first_dev")]
fn apply_first_city_saving_policy(actions: &mut Vec<ValueAction>, state: &State, player: PlayerId) {
    if !should_save_for_first_city_after_first_dev(state, player) {
        return;
    }
    actions.retain(|action| !matches!(action.kind, ValueActionKind::BuyDevCard));
    if actions.is_empty() {
        // Never strand the policy with no legal move.
        actions.push(ValueAction {
            player,
            kind: ValueActionKind::BuyDevCard,
        });
    }
}

#[derive(Clone, Debug)]
pub struct ValueWeights {
    pub public_vps: f64,
    pub production: f64,
    pub enemy_production: f64,
    pub num_tiles: f64,
    pub reachable_production_0: f64,
    pub reachable_production_1: f64,
    pub reachable_production_2: f64,
    pub reachable_production_3: f64,
    pub buildable_nodes: f64,
    pub longest_road: f64,
    pub hand_synergy: f64,
    pub hand_resources: f64,
    pub discard_penalty: f64,
    pub devs_bought: f64,
    pub devs_in_hand_penalty: f64,
    pub army_size: f64,
    pub city_trade_gap: f64,
    pub port_trade: f64,
    pub port_trade_cap: Option<f64>,
}

impl Default for ValueWeights {
    fn default() -> Self {
        Self {
            public_vps: 300000000000000.0,
            production: 100_000_000.0,
            enemy_production: -100_000_000.0,
            num_tiles: 1.0,
            reachable_production_0: 10_000.0,
            reachable_production_1: 2_000.0,
            reachable_production_2: 10.0,
            reachable_production_3: 10.0,
            buildable_nodes: -1000.0,
            longest_road: 10.0,
            hand_synergy: 100.0,
            hand_resources: 1.0,
            discard_penalty: -5_000_000.0,
            devs_bought: 10_000_000.0,
            devs_in_hand_penalty: -20_000.0,
            army_size: 10.1,
            city_trade_gap: 10.0,
            port_trade: 100.0,
            port_trade_cap: Some(0.0),
        }
    }
}

impl ValueWeights {
    pub fn contender() -> Self {
        Self {
            public_vps: 300000000000001.94,
            production: 100000002.04188395,
            enemy_production: -99999998.03389844,
            num_tiles: 2.91440418,
            reachable_production_0: 2.03820085,
            reachable_production_1: 10002.018773150001,
            reachable_production_2: 0.0,
            reachable_production_3: 0.0,
            buildable_nodes: 1001.86278466,
            longest_road: 12.127388499999999,
            hand_synergy: 102.40606877,
            hand_resources: 2.43644327,
            discard_penalty: -3.00141993,
            devs_bought: 0.0,
            devs_in_hand_penalty: 0.0,
            army_size: 12.93844622,
            city_trade_gap: 0.0,
            port_trade: 0.0,
            port_trade_cap: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueActionKind {
    BuildSettlement(NodeId),
    BuildRoad(EdgeId),
    BuildCity(NodeId),
    Roll,
    EndTurn,
    Discard(Option<[u8; RESOURCE_COUNT]>),
    MoveRobber {
        tile: TileId,
        victim: Option<PlayerId>,
        resource: Option<Resource>,
    },
    PlayYearOfPlenty(Resource, Option<Resource>),
    PlayMonopoly(Resource),
    PlayKnight,
    PlayRoadBuilding,
    MaritimeTrade {
        offer: Resource,
        rate: u8,
        ask: Resource,
    },
    BuyDevCard,
    AcceptTrade,
    RejectTrade,
    ConfirmTrade(PlayerId),
    CancelTrade,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueAction {
    pub player: PlayerId,
    pub kind: ValueActionKind,
}

#[derive(Clone, Debug)]
pub struct FastValueFunctionPlayer {
    weights: ValueWeights,
    epsilon: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct ValueComponents {
    pub production: f64,
    pub enemy_production: f64,
    pub reachable_0: f64,
    pub reachable_1: f64,
    pub reachable_2: f64,
    pub reachable_3: f64,
    pub hand_synergy: f64,
    pub num_buildable_nodes: f64,
    pub num_tiles: f64,
    pub num_in_hand: f64,
    pub discard_penalty: f64,
    pub longest_road_length: f64,
    pub longest_road_factor: f64,
    pub city_trade_gap: f64,
    pub port_trade_value: f64,
    pub devs_bought: f64,
    pub devs_in_hand: f64,
    pub vps: f64,
    pub total: f64,
    pub prod_by_res: [f64; RESOURCE_COUNT],
    pub reachable_zero_by_res: [f64; RESOURCE_COUNT],
}

struct SpatialValueCache {
    prod_by_res: [f64; RESOURCE_COUNT],
    production: f64,
    reachability: [[f64; RESOURCE_COUNT]; 4],
    num_buildable_nodes: usize,
    num_tiles: usize,
    player_port_flags: [bool; RESOURCE_COUNT],
    has_three_to_one: bool,
    longest_road_length: f64,
}

struct DecideEvalContext {
    scratch_state: State,
    scratch_road_state: RoadState,
    scratch_army_state: ArmyState,
    lightweight_delta: LightweightDecisionDelta,
    non_structural_delta: NonStructuralDecisionDelta,
    decision_delta: DecisionDelta,
    rollback_mode: RollbackMode,
}

#[derive(Copy, Clone)]
enum RollbackMode {
    Full,
    NonStructural,
    Lightweight,
}

#[derive(Copy, Clone)]
struct DecisionFlagsSnapshot {
    is_initial_build_phase: bool,
    is_discarding: bool,
    is_moving_robber: bool,
    is_road_building: bool,
    is_resolving_trade: bool,
}

#[derive(Clone)]
struct DecisionDelta {
    state_delta: Delta,
    prev_road_state: RoadState,
    prev_army_state: ArmyState,
    prev_prompt: ActionPrompt,
    prev_turn_phase: TurnPhase,
    prev_active_player: PlayerId,
    prev_turn_player: PlayerId,
    prev_robber_tile: TileId,
    prev_flags: DecisionFlagsSnapshot,
    prev_has_rolled: [bool; PLAYER_COUNT],
    prev_has_played_dev: [bool; PLAYER_COUNT],
    prev_dev_owned_at_start: [[bool; DEV_CARD_COUNT]; PLAYER_COUNT],
    prev_free_roads_available: u8,
    prev_trade: [u8; RESOURCE_COUNT * 2],
    prev_acceptees: [bool; PLAYER_COUNT],
    prev_trade_offering_player: PlayerId,
    prev_trade_offered_this_turn: bool,
    prev_road_components: [RoadComponents; PLAYER_COUNT],
    prev_dev_deck: DevDeck,
    prev_dev_cards_in_hand: [[u8; DEV_CARD_COUNT]; PLAYER_COUNT],
    prev_dev_cards_played: [[u8; DEV_CARD_COUNT]; PLAYER_COUNT],
    prev_num_turns: u32,
    prev_last_initial_settlement: [NodeId; PLAYER_COUNT],
    pre_edge_owner: [PlayerId; EDGE_COUNT],
    pre_node_owner: [PlayerId; NODE_COUNT],
    pre_node_level: [BuildingLevel; NODE_COUNT],
    pre_player_resources: [[u8; RESOURCE_COUNT]; PLAYER_COUNT],
    pre_bank_resources: [u8; RESOURCE_COUNT],
}

#[derive(Copy, Clone)]
enum LightweightActionKind {
    BuildSettlement,
    BuildCity,
    BuildRoad,
}

#[derive(Clone)]
struct LightweightDecisionDelta {
    player: PlayerId,
    prev_prompt: ActionPrompt,
    prev_turn_phase: TurnPhase,
    prev_active_player: PlayerId,
    prev_turn_player: PlayerId,
    prev_flags: DecisionFlagsSnapshot,
    prev_free_roads_available: u8,
    prev_num_turns: u32,
    prev_last_initial_settlement: [NodeId; PLAYER_COUNT],
    prev_player_resources: [u8; RESOURCE_COUNT],
    prev_bank_resources: [u8; RESOURCE_COUNT],
    prev_road_state: RoadState,
    prev_node: Option<(NodeId, PlayerId, BuildingLevel)>,
    prev_edge: Option<(EdgeId, PlayerId)>,
    restore_road_components: bool,
    prev_road_components: [RoadComponents; PLAYER_COUNT],
}

#[derive(Clone)]
struct NonStructuralDecisionDelta {
    prev_road_state: RoadState,
    prev_army_state: ArmyState,
    prev_prompt: ActionPrompt,
    prev_turn_phase: TurnPhase,
    prev_active_player: PlayerId,
    prev_turn_player: PlayerId,
    prev_robber_tile: TileId,
    prev_flags: DecisionFlagsSnapshot,
    prev_has_rolled: [bool; PLAYER_COUNT],
    prev_has_played_dev: [bool; PLAYER_COUNT],
    prev_dev_owned_at_start: [[bool; DEV_CARD_COUNT]; PLAYER_COUNT],
    prev_free_roads_available: u8,
    prev_trade: [u8; RESOURCE_COUNT * 2],
    prev_acceptees: [bool; PLAYER_COUNT],
    prev_trade_offering_player: PlayerId,
    prev_trade_offered_this_turn: bool,
    prev_dev_deck: DevDeck,
    prev_dev_cards_in_hand: [[u8; DEV_CARD_COUNT]; PLAYER_COUNT],
    prev_dev_cards_played: [[u8; DEV_CARD_COUNT]; PLAYER_COUNT],
    prev_num_turns: u32,
    prev_last_initial_settlement: [NodeId; PLAYER_COUNT],
    pre_player_resources: [[u8; RESOURCE_COUNT]; PLAYER_COUNT],
    pre_bank_resources: [u8; RESOURCE_COUNT],
}

impl LightweightDecisionDelta {
    fn new() -> Self {
        Self {
            player: 0,
            prev_prompt: ActionPrompt::PlayTurn,
            prev_turn_phase: TurnPhase::Setup,
            prev_active_player: 0,
            prev_turn_player: 0,
            prev_flags: DecisionFlagsSnapshot {
                is_initial_build_phase: false,
                is_discarding: false,
                is_moving_robber: false,
                is_road_building: false,
                is_resolving_trade: false,
            },
            prev_free_roads_available: 0,
            prev_num_turns: 0,
            prev_last_initial_settlement: [INVALID_NODE; PLAYER_COUNT],
            prev_player_resources: [0; RESOURCE_COUNT],
            prev_bank_resources: [0; RESOURCE_COUNT],
            prev_road_state: RoadState::empty(),
            prev_node: None,
            prev_edge: None,
            restore_road_components: false,
            prev_road_components: std::array::from_fn(|_| RoadComponents::new()),
        }
    }

    fn capture_for_action(
        &mut self,
        state: &State,
        road_state: &RoadState,
        action: &ValueAction,
    ) -> bool {
        let (kind, node, edge) = match action.kind {
            ValueActionKind::BuildSettlement(node) => {
                (LightweightActionKind::BuildSettlement, Some(node), None)
            }
            ValueActionKind::BuildCity(node) => {
                (LightweightActionKind::BuildCity, Some(node), None)
            }
            ValueActionKind::BuildRoad(edge) => {
                (LightweightActionKind::BuildRoad, None, Some(edge))
            }
            _ => return false,
        };

        self.player = action.player;
        self.prev_prompt = state.current_prompt;
        self.prev_turn_phase = state.turn_phase;
        self.prev_active_player = state.active_player;
        self.prev_turn_player = state.turn_player;
        self.prev_flags = DecisionFlagsSnapshot {
            is_initial_build_phase: state.is_initial_build_phase,
            is_discarding: state.is_discarding,
            is_moving_robber: state.is_moving_robber,
            is_road_building: state.is_road_building,
            is_resolving_trade: state.is_resolving_trade,
        };
        self.prev_free_roads_available = state.free_roads_available;
        self.prev_num_turns = state.num_turns;
        self.prev_last_initial_settlement = state.last_initial_settlement;
        self.prev_player_resources = state.player_resources[action.player as usize];
        self.prev_bank_resources = state.bank_resources;
        self.prev_road_state = *road_state;
        self.prev_node = node.map(|node_id| {
            let idx = node_id as usize;
            (node_id, state.node_owner[idx], state.node_level[idx])
        });
        self.prev_edge = edge.map(|edge_id| (edge_id, state.edge_owner[edge_id as usize]));
        self.restore_road_components = matches!(
            kind,
            LightweightActionKind::BuildSettlement | LightweightActionKind::BuildRoad
        );
        if self.restore_road_components {
            self.prev_road_components = state.road_components;
        }
        true
    }

    fn undo(&self, state: &mut State, road_state: &mut RoadState) {
        if let Some((node, owner, level)) = self.prev_node {
            let idx = node as usize;
            state.node_owner[idx] = owner;
            state.node_level[idx] = level;
        }
        if let Some((edge, owner)) = self.prev_edge {
            state.edge_owner[edge as usize] = owner;
        }
        state.player_resources[self.player as usize] = self.prev_player_resources;
        state.bank_resources = self.prev_bank_resources;
        *road_state = self.prev_road_state;
        if self.restore_road_components {
            state.road_components = self.prev_road_components;
        }
        state.current_prompt = self.prev_prompt;
        state.turn_phase = self.prev_turn_phase;
        state.active_player = self.prev_active_player;
        state.turn_player = self.prev_turn_player;
        state.is_initial_build_phase = self.prev_flags.is_initial_build_phase;
        state.is_discarding = self.prev_flags.is_discarding;
        state.is_moving_robber = self.prev_flags.is_moving_robber;
        state.is_road_building = self.prev_flags.is_road_building;
        state.is_resolving_trade = self.prev_flags.is_resolving_trade;
        state.free_roads_available = self.prev_free_roads_available;
        state.num_turns = self.prev_num_turns;
        state.last_initial_settlement = self.prev_last_initial_settlement;
    }
}

impl NonStructuralDecisionDelta {
    fn new() -> Self {
        Self {
            prev_road_state: RoadState::empty(),
            prev_army_state: ArmyState::empty(),
            prev_prompt: ActionPrompt::PlayTurn,
            prev_turn_phase: TurnPhase::Setup,
            prev_active_player: 0,
            prev_turn_player: 0,
            prev_robber_tile: INVALID_TILE,
            prev_flags: DecisionFlagsSnapshot {
                is_initial_build_phase: false,
                is_discarding: false,
                is_moving_robber: false,
                is_road_building: false,
                is_resolving_trade: false,
            },
            prev_has_rolled: [false; PLAYER_COUNT],
            prev_has_played_dev: [false; PLAYER_COUNT],
            prev_dev_owned_at_start: [[false; DEV_CARD_COUNT]; PLAYER_COUNT],
            prev_free_roads_available: 0,
            prev_trade: [0; RESOURCE_COUNT * 2],
            prev_acceptees: [false; PLAYER_COUNT],
            prev_trade_offering_player: 0,
            prev_trade_offered_this_turn: false,
            prev_dev_deck: DevDeck::default(),
            prev_dev_cards_in_hand: [[0; DEV_CARD_COUNT]; PLAYER_COUNT],
            prev_dev_cards_played: [[0; DEV_CARD_COUNT]; PLAYER_COUNT],
            prev_num_turns: 0,
            prev_last_initial_settlement: [INVALID_NODE; PLAYER_COUNT],
            pre_player_resources: [[0; RESOURCE_COUNT]; PLAYER_COUNT],
            pre_bank_resources: [0; RESOURCE_COUNT],
        }
    }

    #[inline]
    fn begin(&mut self, state: &State, road_state: &RoadState, army_state: &ArmyState) {
        self.prev_road_state = *road_state;
        self.prev_army_state = *army_state;
        self.prev_prompt = state.current_prompt;
        self.prev_turn_phase = state.turn_phase;
        self.prev_active_player = state.active_player;
        self.prev_turn_player = state.turn_player;
        self.prev_robber_tile = state.robber_tile;
        self.prev_flags = DecisionFlagsSnapshot {
            is_initial_build_phase: state.is_initial_build_phase,
            is_discarding: state.is_discarding,
            is_moving_robber: state.is_moving_robber,
            is_road_building: state.is_road_building,
            is_resolving_trade: state.is_resolving_trade,
        };
        self.prev_has_rolled = state.has_rolled;
        self.prev_has_played_dev = state.has_played_dev;
        self.prev_dev_owned_at_start = state.dev_owned_at_start;
        self.prev_free_roads_available = state.free_roads_available;
        self.prev_trade = state.current_trade;
        self.prev_acceptees = state.acceptees;
        self.prev_trade_offering_player = state.trade_offering_player;
        self.prev_trade_offered_this_turn = state.trade_offered_this_turn;
        self.prev_dev_deck = state.dev_deck;
        self.prev_dev_cards_in_hand = state.dev_cards_in_hand;
        self.prev_dev_cards_played = state.dev_cards_played;
        self.prev_num_turns = state.num_turns;
        self.prev_last_initial_settlement = state.last_initial_settlement;
        self.pre_player_resources = state.player_resources;
        self.pre_bank_resources = state.bank_resources;
    }

    #[inline]
    fn undo(&self, state: &mut State, road_state: &mut RoadState, army_state: &mut ArmyState) {
        state.player_resources = self.pre_player_resources;
        state.bank_resources = self.pre_bank_resources;
        *road_state = self.prev_road_state;
        *army_state = self.prev_army_state;
        state.current_prompt = self.prev_prompt;
        state.turn_phase = self.prev_turn_phase;
        state.active_player = self.prev_active_player;
        state.turn_player = self.prev_turn_player;
        state.robber_tile = self.prev_robber_tile;
        state.is_initial_build_phase = self.prev_flags.is_initial_build_phase;
        state.is_discarding = self.prev_flags.is_discarding;
        state.is_moving_robber = self.prev_flags.is_moving_robber;
        state.is_road_building = self.prev_flags.is_road_building;
        state.is_resolving_trade = self.prev_flags.is_resolving_trade;
        state.has_rolled = self.prev_has_rolled;
        state.has_played_dev = self.prev_has_played_dev;
        state.dev_owned_at_start = self.prev_dev_owned_at_start;
        state.free_roads_available = self.prev_free_roads_available;
        state.current_trade = self.prev_trade;
        state.acceptees = self.prev_acceptees;
        state.trade_offering_player = self.prev_trade_offering_player;
        state.trade_offered_this_turn = self.prev_trade_offered_this_turn;
        state.dev_deck = self.prev_dev_deck;
        state.dev_cards_in_hand = self.prev_dev_cards_in_hand;
        state.dev_cards_played = self.prev_dev_cards_played;
        state.num_turns = self.prev_num_turns;
        state.last_initial_settlement = self.prev_last_initial_settlement;
    }
}

impl DecisionDelta {
    fn new() -> Self {
        Self {
            state_delta: Delta::default(),
            prev_road_state: RoadState::empty(),
            prev_army_state: ArmyState::empty(),
            prev_prompt: ActionPrompt::PlayTurn,
            prev_turn_phase: TurnPhase::Setup,
            prev_active_player: 0,
            prev_turn_player: 0,
            prev_robber_tile: INVALID_TILE,
            prev_flags: DecisionFlagsSnapshot {
                is_initial_build_phase: false,
                is_discarding: false,
                is_moving_robber: false,
                is_road_building: false,
                is_resolving_trade: false,
            },
            prev_has_rolled: [false; PLAYER_COUNT],
            prev_has_played_dev: [false; PLAYER_COUNT],
            prev_dev_owned_at_start: [[false; DEV_CARD_COUNT]; PLAYER_COUNT],
            prev_free_roads_available: 0,
            prev_trade: [0; RESOURCE_COUNT * 2],
            prev_acceptees: [false; PLAYER_COUNT],
            prev_trade_offering_player: 0,
            prev_trade_offered_this_turn: false,
            prev_road_components: std::array::from_fn(|_| RoadComponents::new()),
            prev_dev_deck: DevDeck::default(),
            prev_dev_cards_in_hand: [[0; DEV_CARD_COUNT]; PLAYER_COUNT],
            prev_dev_cards_played: [[0; DEV_CARD_COUNT]; PLAYER_COUNT],
            prev_num_turns: 0,
            prev_last_initial_settlement: [INVALID_NODE; PLAYER_COUNT],
            pre_edge_owner: [NO_PLAYER; EDGE_COUNT],
            pre_node_owner: [NO_PLAYER; NODE_COUNT],
            pre_node_level: [BuildingLevel::Empty; NODE_COUNT],
            pre_player_resources: [[0; RESOURCE_COUNT]; PLAYER_COUNT],
            pre_bank_resources: [0; RESOURCE_COUNT],
        }
    }

    #[inline]
    fn begin(&mut self, state: &State, road_state: &RoadState, army_state: &ArmyState) {
        self.state_delta.reset();
        self.prev_road_state = *road_state;
        self.prev_army_state = *army_state;
        self.prev_prompt = state.current_prompt;
        self.prev_turn_phase = state.turn_phase;
        self.prev_active_player = state.active_player;
        self.prev_turn_player = state.turn_player;
        self.prev_robber_tile = state.robber_tile;
        self.prev_flags = DecisionFlagsSnapshot {
            is_initial_build_phase: state.is_initial_build_phase,
            is_discarding: state.is_discarding,
            is_moving_robber: state.is_moving_robber,
            is_road_building: state.is_road_building,
            is_resolving_trade: state.is_resolving_trade,
        };
        self.prev_has_rolled = state.has_rolled;
        self.prev_has_played_dev = state.has_played_dev;
        self.prev_dev_owned_at_start = state.dev_owned_at_start;
        self.prev_free_roads_available = state.free_roads_available;
        self.prev_trade = state.current_trade;
        self.prev_acceptees = state.acceptees;
        self.prev_trade_offering_player = state.trade_offering_player;
        self.prev_trade_offered_this_turn = state.trade_offered_this_turn;
        self.prev_road_components = state.road_components;
        self.prev_dev_deck = state.dev_deck;
        self.prev_dev_cards_in_hand = state.dev_cards_in_hand;
        self.prev_dev_cards_played = state.dev_cards_played;
        self.prev_num_turns = state.num_turns;
        self.prev_last_initial_settlement = state.last_initial_settlement;
        self.pre_edge_owner = state.edge_owner;
        self.pre_node_owner = state.node_owner;
        self.pre_node_level = state.node_level;
        self.pre_player_resources = state.player_resources;
        self.pre_bank_resources = state.bank_resources;
    }

    #[inline]
    fn record_core_diffs(&mut self, state: &State) {
        for edge in 0..EDGE_COUNT {
            let before = self.pre_edge_owner[edge];
            let after = state.edge_owner[edge];
            if before != after {
                self.state_delta.record_road(edge as EdgeId, before);
            }
        }

        for node in 0..NODE_COUNT {
            let owner_before = self.pre_node_owner[node];
            let level_before = self.pre_node_level[node];
            let owner_after = state.node_owner[node];
            let level_after = state.node_level[node];
            if owner_before != owner_after || level_before != level_after {
                self.state_delta
                    .record_building(node as NodeId, owner_before, level_before);
            }
        }

        for player in 0..PLAYER_COUNT {
            for resource in 0..RESOURCE_COUNT {
                let before = self.pre_player_resources[player][resource];
                let after = state.player_resources[player][resource];
                if before != after {
                    if let Some(resource_id) = Resource::from_index(resource) {
                        self.state_delta
                            .record_resource(player as PlayerId, resource_id, before);
                    }
                }
            }
        }

        for resource in 0..RESOURCE_COUNT {
            let before = self.pre_bank_resources[resource];
            let after = state.bank_resources[resource];
            if before != after {
                if let Some(resource_id) = Resource::from_index(resource) {
                    self.state_delta.record_bank(resource_id, before);
                }
            }
        }

        if self.prev_turn_player != state.turn_player || self.prev_turn_phase != state.turn_phase {
            self.state_delta
                .record_turn(self.prev_turn_player, self.prev_turn_phase);
        }
        if self.prev_robber_tile != state.robber_tile {
            self.state_delta.record_robber(self.prev_robber_tile);
        }
    }
}

impl DecideEvalContext {
    fn new(state: &State, road_state: &RoadState, army_state: &ArmyState) -> Self {
        Self {
            scratch_state: state.clone(),
            scratch_road_state: *road_state,
            scratch_army_state: *army_state,
            lightweight_delta: LightweightDecisionDelta::new(),
            non_structural_delta: NonStructuralDecisionDelta::new(),
            decision_delta: DecisionDelta::new(),
            rollback_mode: RollbackMode::Full,
        }
    }

    #[inline]
    fn apply(&mut self, board: &Board, action: &ValueAction, rng: &mut impl RngCore) {
        if self.lightweight_delta.capture_for_action(
            &self.scratch_state,
            &self.scratch_road_state,
            action,
        ) {
            apply_value_action_kernel(
                board,
                &mut self.scratch_state,
                &mut self.scratch_road_state,
                &mut self.scratch_army_state,
                action,
                rng,
            );
            self.rollback_mode = RollbackMode::Lightweight;
            return;
        }

        if action_is_non_structural(&action.kind) {
            self.non_structural_delta.begin(
                &self.scratch_state,
                &self.scratch_road_state,
                &self.scratch_army_state,
            );
            apply_value_action_kernel(
                board,
                &mut self.scratch_state,
                &mut self.scratch_road_state,
                &mut self.scratch_army_state,
                action,
                rng,
            );
            self.rollback_mode = RollbackMode::NonStructural;
            return;
        }

        apply_value_action_reversible(
            board,
            &mut self.scratch_state,
            &mut self.scratch_road_state,
            &mut self.scratch_army_state,
            action,
            rng,
            &mut self.decision_delta,
        );
        self.rollback_mode = RollbackMode::Full;
    }

    #[inline]
    fn rollback(&mut self) {
        match self.rollback_mode {
            RollbackMode::Full => {
                undo_value_action_reversible(
                    &mut self.scratch_state,
                    &mut self.scratch_road_state,
                    &mut self.scratch_army_state,
                    &self.decision_delta,
                );
            }
            RollbackMode::Lightweight => {
                self.lightweight_delta
                    .undo(&mut self.scratch_state, &mut self.scratch_road_state);
            }
            RollbackMode::NonStructural => {
                self.non_structural_delta.undo(
                    &mut self.scratch_state,
                    &mut self.scratch_road_state,
                    &mut self.scratch_army_state,
                );
            }
        }
    }
}

#[inline]
fn apply_value_action_reversible(
    board: &Board,
    state: &mut State,
    road_state: &mut RoadState,
    army_state: &mut ArmyState,
    action: &ValueAction,
    rng: &mut impl RngCore,
    decision_delta: &mut DecisionDelta,
) {
    decision_delta.begin(state, road_state, army_state);
    apply_value_action_kernel(board, state, road_state, army_state, action, rng);
    decision_delta.record_core_diffs(state);
}

#[inline]
fn undo_value_action_reversible(
    state: &mut State,
    road_state: &mut RoadState,
    army_state: &mut ArmyState,
    decision_delta: &DecisionDelta,
) {
    state.undo(&decision_delta.state_delta);
    *road_state = decision_delta.prev_road_state;
    *army_state = decision_delta.prev_army_state;
    state.current_prompt = decision_delta.prev_prompt;
    state.turn_phase = decision_delta.prev_turn_phase;
    state.active_player = decision_delta.prev_active_player;
    state.turn_player = decision_delta.prev_turn_player;
    state.robber_tile = decision_delta.prev_robber_tile;
    state.is_initial_build_phase = decision_delta.prev_flags.is_initial_build_phase;
    state.is_discarding = decision_delta.prev_flags.is_discarding;
    state.is_moving_robber = decision_delta.prev_flags.is_moving_robber;
    state.is_road_building = decision_delta.prev_flags.is_road_building;
    state.is_resolving_trade = decision_delta.prev_flags.is_resolving_trade;
    state.has_rolled = decision_delta.prev_has_rolled;
    state.has_played_dev = decision_delta.prev_has_played_dev;
    state.dev_owned_at_start = decision_delta.prev_dev_owned_at_start;
    state.free_roads_available = decision_delta.prev_free_roads_available;
    state.current_trade = decision_delta.prev_trade;
    state.acceptees = decision_delta.prev_acceptees;
    state.trade_offering_player = decision_delta.prev_trade_offering_player;
    state.trade_offered_this_turn = decision_delta.prev_trade_offered_this_turn;
    state.road_components = decision_delta.prev_road_components;
    state.dev_deck = decision_delta.prev_dev_deck;
    state.dev_cards_in_hand = decision_delta.prev_dev_cards_in_hand;
    state.dev_cards_played = decision_delta.prev_dev_cards_played;
    state.num_turns = decision_delta.prev_num_turns;
    state.last_initial_settlement = decision_delta.prev_last_initial_settlement;
}

impl FastValueFunctionPlayer {
    pub fn new(weights: Option<ValueWeights>, epsilon: Option<f64>) -> Self {
        Self {
            weights: weights.unwrap_or_default(),
            epsilon,
        }
    }

    pub fn contender(epsilon: Option<f64>) -> Self {
        Self {
            weights: ValueWeights::contender(),
            epsilon,
        }
    }

    pub fn decide(
        &self,
        board: &Board,
        state: &State,
        road_state: &RoadState,
        army_state: &ArmyState,
        rng: &mut impl RngCore,
    ) -> ValueAction {
        if std::env::var_os("FASTCORE_DECIDE_USE_CLONE").is_some() {
            return self.decide_with_clone_eval(board, state, road_state, army_state, rng);
        }

        let player = state.active_player;
        let mut actions = generate_playable_actions(board, state, player);
        if actions.is_empty() {
            return ValueAction {
                player,
                kind: ValueActionKind::EndTurn,
            };
        }

        if state.current_prompt == ActionPrompt::MoveRobber && !cfg!(feature = "legacy_robber") {
            if let Some(action) =
                leader_robber_action(board, state, road_state, army_state, player, &actions)
            {
                return action;
            }
            let has_non_blocking = actions.iter().any(|action| match action.kind {
                ValueActionKind::MoveRobber { tile, .. } => {
                    !player_has_building_on_tile(board, state, player, tile)
                }
                _ => true,
            });
            if has_non_blocking {
                actions.retain(|action| match action.kind {
                    ValueActionKind::MoveRobber { tile, .. } => {
                        !player_has_building_on_tile(board, state, player, tile)
                    }
                    _ => true,
                });
            }
        }

        let has_building_action = actions.iter().any(|action| {
            matches!(
                action.kind,
                ValueActionKind::BuildSettlement(_) | ValueActionKind::BuildCity(_)
            )
        });

        if !has_building_action {
            if let Some(action) = actions
                .iter()
                .find(|action| matches!(action.kind, ValueActionKind::PlayRoadBuilding))
            {
                return action.clone();
            }
        }

        if has_building_action {
            actions.retain(|action| {
                matches!(
                    action.kind,
                    ValueActionKind::BuildSettlement(_) | ValueActionKind::BuildCity(_)
                )
            });
        }
        #[cfg(feature = "save_first_city_after_first_dev")]
        apply_first_city_saving_policy(&mut actions, state, player);

        if actions.len() == 1 {
            return actions[0].clone();
        }

        if let Some(epsilon) = self.epsilon {
            let roll = rng.next_u64() as f64 / (u64::MAX as f64 + 1.0);
            if roll < epsilon {
                let idx = next_u64_mod(rng, actions.len() as u64) as usize;
                return actions[idx].clone();
            }
        }

        actions.sort_by(compare_value_actions);

        let mut best_value = f64::NEG_INFINITY;
        let mut best_action = actions[0].clone();
        let baseline_enemy_production = enemy_production_total(board, state, player);
        let spatial_cache = build_spatial_value_cache(board, state, road_state, player);
        let mut eval = DecideEvalContext::new(state, road_state, army_state);
        for action in actions {
            eval.apply(board, &action, rng);
            let enemy_override = if action_changes_enemy_production(&action.kind) {
                None
            } else {
                Some(baseline_enemy_production)
            };
            let value = if action_reuses_spatial_cache(&action.kind) {
                self.value_with_cached_spatial(
                    &eval.scratch_state,
                    &eval.scratch_road_state,
                    &eval.scratch_army_state,
                    player,
                    baseline_enemy_production,
                    &spatial_cache,
                )
            } else {
                self.value_with_enemy_override(
                    board,
                    &eval.scratch_state,
                    &eval.scratch_road_state,
                    &eval.scratch_army_state,
                    player,
                    enemy_override,
                )
            };
            eval.rollback();
            if value > best_value {
                best_value = value;
                best_action = action;
            }
        }

        best_action
    }

    fn decide_with_clone_eval(
        &self,
        board: &Board,
        state: &State,
        road_state: &RoadState,
        army_state: &ArmyState,
        rng: &mut impl RngCore,
    ) -> ValueAction {
        let player = state.active_player;
        let mut actions = generate_playable_actions(board, state, player);
        if actions.is_empty() {
            return ValueAction {
                player,
                kind: ValueActionKind::EndTurn,
            };
        }

        if state.current_prompt == ActionPrompt::MoveRobber && !cfg!(feature = "legacy_robber") {
            if let Some(action) =
                leader_robber_action(board, state, road_state, army_state, player, &actions)
            {
                return action;
            }
            let has_non_blocking = actions.iter().any(|action| match action.kind {
                ValueActionKind::MoveRobber { tile, .. } => {
                    !player_has_building_on_tile(board, state, player, tile)
                }
                _ => true,
            });
            if has_non_blocking {
                actions.retain(|action| match action.kind {
                    ValueActionKind::MoveRobber { tile, .. } => {
                        !player_has_building_on_tile(board, state, player, tile)
                    }
                    _ => true,
                });
            }
        }

        let has_building_action = actions.iter().any(|action| {
            matches!(
                action.kind,
                ValueActionKind::BuildSettlement(_) | ValueActionKind::BuildCity(_)
            )
        });

        if !has_building_action {
            if let Some(action) = actions
                .iter()
                .find(|action| matches!(action.kind, ValueActionKind::PlayRoadBuilding))
            {
                return action.clone();
            }
        }

        if has_building_action {
            actions.retain(|action| {
                matches!(
                    action.kind,
                    ValueActionKind::BuildSettlement(_) | ValueActionKind::BuildCity(_)
                )
            });
        }
        #[cfg(feature = "save_first_city_after_first_dev")]
        apply_first_city_saving_policy(&mut actions, state, player);

        if actions.len() == 1 {
            return actions[0].clone();
        }

        if let Some(epsilon) = self.epsilon {
            let roll = rng.next_u64() as f64 / (u64::MAX as f64 + 1.0);
            if roll < epsilon {
                let idx = next_u64_mod(rng, actions.len() as u64) as usize;
                return actions[idx].clone();
            }
        }

        actions.sort_by(compare_value_actions);

        let mut best_value = f64::NEG_INFINITY;
        let mut best_action = actions[0].clone();
        for action in actions {
            let mut state_copy = state.clone();
            let mut road_copy = *road_state;
            let mut army_copy = *army_state;
            apply_value_action(
                board,
                &mut state_copy,
                &mut road_copy,
                &mut army_copy,
                &action,
                rng,
            );
            let value = self.value(board, &state_copy, &road_copy, &army_copy, player);
            if value > best_value {
                best_value = value;
                best_action = action;
            }
        }

        best_action
    }

    pub fn value(
        &self,
        board: &Board,
        state: &State,
        road_state: &RoadState,
        army_state: &ArmyState,
        player: PlayerId,
    ) -> f64 {
        self.value_with_enemy_override(board, state, road_state, army_state, player, None)
    }

    fn value_with_enemy_override(
        &self,
        board: &Board,
        state: &State,
        road_state: &RoadState,
        army_state: &ArmyState,
        player: PlayerId,
        enemy_production_override: Option<f64>,
    ) -> f64 {
        self.value_components_with_enemy_override(
            board,
            state,
            road_state,
            army_state,
            player,
            enemy_production_override,
        )
        .total
    }

    fn value_with_cached_spatial(
        &self,
        state: &State,
        road_state: &RoadState,
        army_state: &ArmyState,
        player: PlayerId,
        enemy_production: f64,
        cache: &SpatialValueCache,
    ) -> f64 {
        let weights = &self.weights;
        let public_vps_value = public_vps(state, road_state, army_state, player) as f64;
        let endgame = endgame_gap_enabled() && public_vps_value >= 7.0;
        let endgame_multiplier = if endgame { 0.2 } else { 1.0 };

        let reachable_at_zero = cache.reachability[0];
        let reachable_at_one = cache.reachability[1];
        let reachable_at_two = cache.reachability[2];
        let reachable_at_three = cache.reachability[3];

        let hand_counts = state.player_resources[player as usize];
        let wheat_in_hand = hand_counts[Resource::Grain.as_index()];
        let ore_in_hand = hand_counts[Resource::Ore.as_index()];
        let sheep_in_hand = hand_counts[Resource::Wool.as_index()];
        let wood_in_hand = hand_counts[Resource::Lumber.as_index()];
        let brick_in_hand = hand_counts[Resource::Brick.as_index()];

        let distance_to_city = ((2_i32 - wheat_in_hand as i32).max(0) as f64
            + (3_i32 - ore_in_hand as i32).max(0) as f64)
            / 5.0;
        let distance_to_settlement = ((1_i32 - wheat_in_hand as i32).max(0) as f64
            + (1_i32 - sheep_in_hand as i32).max(0) as f64
            + (1_i32 - brick_in_hand as i32).max(0) as f64
            + (1_i32 - wood_in_hand as i32).max(0) as f64)
            / 4.0;
        let hand_synergy = (2.0 - distance_to_city - distance_to_settlement) / 2.0;

        let num_in_hand = player_resource_total(state, player) as f64;
        let discard_penalty = if num_in_hand > 7.0 {
            (num_in_hand - 7.0) * weights.discard_penalty
        } else {
            0.0
        };

        let num_tiles = cache.num_tiles as f64;
        let num_buildable_nodes = cache.num_buildable_nodes as f64;
        let longest_road_factor = if num_buildable_nodes == 0.0 {
            weights.longest_road
        } else {
            0.1
        };
        let longest_road_factor = if endgame {
            longest_road_factor * endgame_multiplier
        } else {
            longest_road_factor
        };

        let ore_needed = (3_i32 - ore_in_hand as i32).max(0) as f64;
        let wheat_needed = (2_i32 - wheat_in_hand as i32).max(0) as f64;
        let missing_city_cards = ore_needed + wheat_needed;

        let mut tradable_cover = 0.0;
        for resource in RESOURCE_ORDER {
            let in_hand = hand_counts[resource.as_index()] as f64;
            let surplus = match resource {
                Resource::Ore => (in_hand - 3.0).max(0.0),
                Resource::Grain => (in_hand - 2.0).max(0.0),
                _ => in_hand,
            };
            let mut rate = 4.0;
            if cache.has_three_to_one {
                rate = 3.0;
            }
            if cache.player_port_flags[resource.as_index()] {
                rate = 2.0;
            }
            tradable_cover += surplus / rate;
        }
        let city_trade_gap = ((missing_city_cards - tradable_cover) / 5.0).max(0.0);

        let base_trade_rate = if cache.has_three_to_one {
            1.0 / 3.0
        } else {
            1.0 / 4.0
        };
        let mut port_trade_value = 0.0;
        for resource in RESOURCE_ORDER {
            let res_idx = resource.as_index();
            if !cache.player_port_flags[res_idx] {
                continue;
            }
            let upgrade_gain =
                (cache.prod_by_res[res_idx] + reachable_at_zero[res_idx]) * (0.5 - base_trade_rate);
            if upgrade_gain > 0.0 {
                port_trade_value += upgrade_gain;
            }
        }
        if let Some(cap) = weights.port_trade_cap {
            port_trade_value = port_trade_value.min(cap);
        }

        let devs_in_hand = state.dev_cards_in_hand[player as usize]
            .iter()
            .map(|count| *count as f64)
            .sum::<f64>();
        let devs_played = state.dev_cards_played[player as usize]
            .iter()
            .map(|count| *count as f64)
            .sum::<f64>();
        let devs_bought = devs_in_hand + devs_played;
        let knights_played = state.dev_cards_played[player as usize][DevCard::Knight.as_index()];

        let vps = public_vps_value;
        let build_gap = distance_to_city + distance_to_settlement;
        let endgame_gap_term = if endgame {
            -build_gap * ENDGAME_GAP_WEIGHT
        } else {
            0.0
        };

        vps * weights.public_vps
            + cache.production * weights.production * endgame_multiplier
            + enemy_production * weights.enemy_production * endgame_multiplier
            + sum_resources(&reachable_at_zero)
                * weights.reachable_production_0
                * endgame_multiplier
            + sum_resources(&reachable_at_one) * weights.reachable_production_1 * endgame_multiplier
            + sum_resources(&reachable_at_two) * weights.reachable_production_2 * endgame_multiplier
            + sum_resources(&reachable_at_three)
                * weights.reachable_production_3
                * endgame_multiplier
            - city_trade_gap * weights.city_trade_gap
            + hand_synergy * weights.hand_synergy
            + num_buildable_nodes * weights.buildable_nodes * endgame_multiplier
            + num_tiles * weights.num_tiles * endgame_multiplier
            + num_in_hand * weights.hand_resources
            + discard_penalty
            + cache.longest_road_length * longest_road_factor
            + (knights_played as f64) * weights.army_size
            + port_trade_value * weights.port_trade * endgame_multiplier
            + devs_bought * weights.devs_bought
            + devs_in_hand * weights.devs_in_hand_penalty
            + endgame_gap_term
    }

    pub fn value_components(
        &self,
        board: &Board,
        state: &State,
        road_state: &RoadState,
        army_state: &ArmyState,
        player: PlayerId,
    ) -> ValueComponents {
        self.value_components_with_enemy_override(
            board, state, road_state, army_state, player, None,
        )
    }

    fn value_components_with_enemy_override(
        &self,
        board: &Board,
        state: &State,
        road_state: &RoadState,
        army_state: &ArmyState,
        player: PlayerId,
        enemy_production_override: Option<f64>,
    ) -> ValueComponents {
        let weights = &self.weights;
        let public_vps_value = public_vps(state, road_state, army_state, player) as f64;
        let endgame = endgame_gap_enabled() && public_vps_value >= 7.0;
        let endgame_multiplier = if endgame { 0.2 } else { 1.0 };

        let prod_by_res = production_by_resource(board, state, player, true);
        let production = value_production(&prod_by_res, true);

        let enemy_production = enemy_production_override
            .unwrap_or_else(|| enemy_production_total(board, state, player));

        let (reachability, num_buildable_nodes) = reachability_no_enemy(board, state, player);
        let reachable_at_zero = reachability[0];
        let reachable_at_one = reachability[1];
        let reachable_at_two = reachability[2];
        let reachable_at_three = reachability[3];

        let hand_counts = state.player_resources[player as usize];
        let wheat_in_hand = hand_counts[Resource::Grain.as_index()];
        let ore_in_hand = hand_counts[Resource::Ore.as_index()];
        let sheep_in_hand = hand_counts[Resource::Wool.as_index()];
        let wood_in_hand = hand_counts[Resource::Lumber.as_index()];
        let brick_in_hand = hand_counts[Resource::Brick.as_index()];

        let distance_to_city = ((2_i32 - wheat_in_hand as i32).max(0) as f64
            + (3_i32 - ore_in_hand as i32).max(0) as f64)
            / 5.0;
        let distance_to_settlement = ((1_i32 - wheat_in_hand as i32).max(0) as f64
            + (1_i32 - sheep_in_hand as i32).max(0) as f64
            + (1_i32 - brick_in_hand as i32).max(0) as f64
            + (1_i32 - wood_in_hand as i32).max(0) as f64)
            / 4.0;
        let hand_synergy = (2.0 - distance_to_city - distance_to_settlement) / 2.0;

        let num_in_hand = player_resource_total(state, player) as f64;
        let discard_penalty = if num_in_hand > 7.0 {
            (num_in_hand - 7.0) * weights.discard_penalty
        } else {
            0.0
        };

        let (num_tiles, player_port_flags, has_three_to_one) =
            player_node_features(board, state, player);
        let num_tiles = num_tiles as f64;
        let num_buildable_nodes = num_buildable_nodes as f64;
        let longest_road_factor = if num_buildable_nodes == 0.0 {
            weights.longest_road
        } else {
            0.1
        };
        let longest_road_factor = if endgame {
            longest_road_factor * endgame_multiplier
        } else {
            longest_road_factor
        };
        let longest_road_length = road_state.length_for_player(player) as f64;

        let ore_needed = (3_i32 - ore_in_hand as i32).max(0) as f64;
        let wheat_needed = (2_i32 - wheat_in_hand as i32).max(0) as f64;
        let missing_city_cards = ore_needed + wheat_needed;

        let mut tradable_cover = 0.0;
        for resource in RESOURCE_ORDER {
            let in_hand = hand_counts[resource.as_index()] as f64;
            let surplus = match resource {
                Resource::Ore => (in_hand - 3.0).max(0.0),
                Resource::Grain => (in_hand - 2.0).max(0.0),
                _ => in_hand,
            };
            let mut rate = 4.0;
            if has_three_to_one {
                rate = 3.0;
            }
            if player_port_flags[resource.as_index()] {
                rate = 2.0;
            }
            tradable_cover += surplus / rate;
        }

        let city_trade_gap = ((missing_city_cards - tradable_cover) / 5.0).max(0.0);

        let base_trade_rate = if has_three_to_one {
            1.0 / 3.0
        } else {
            1.0 / 4.0
        };
        let mut port_trade_value = 0.0;
        for resource in RESOURCE_ORDER {
            let res_idx = resource.as_index();
            if !player_port_flags[res_idx] {
                continue;
            }
            let upgrade_gain =
                (prod_by_res[res_idx] + reachable_at_zero[res_idx]) * (0.5 - base_trade_rate);
            if upgrade_gain > 0.0 {
                port_trade_value += upgrade_gain;
            }
        }
        if let Some(cap) = weights.port_trade_cap {
            port_trade_value = port_trade_value.min(cap);
        }

        let devs_in_hand = state.dev_cards_in_hand[player as usize]
            .iter()
            .map(|count| *count as f64)
            .sum::<f64>();
        let devs_played = state.dev_cards_played[player as usize]
            .iter()
            .map(|count| *count as f64)
            .sum::<f64>();
        let devs_bought = devs_in_hand + devs_played;

        let knights_played = state.dev_cards_played[player as usize][DevCard::Knight.as_index()];

        let vps = public_vps_value;
        let build_gap = distance_to_city + distance_to_settlement;
        let endgame_gap_term = if endgame {
            -build_gap * ENDGAME_GAP_WEIGHT
        } else {
            0.0
        };

        let total = vps * weights.public_vps
            + production * weights.production * endgame_multiplier
            + enemy_production * weights.enemy_production * endgame_multiplier
            + sum_resources(&reachable_at_zero)
                * weights.reachable_production_0
                * endgame_multiplier
            + sum_resources(&reachable_at_one)
                * weights.reachable_production_1
                * endgame_multiplier
            + sum_resources(&reachable_at_two)
                * weights.reachable_production_2
                * endgame_multiplier
            + sum_resources(&reachable_at_three)
                * weights.reachable_production_3
                * endgame_multiplier
            - city_trade_gap * weights.city_trade_gap
            + hand_synergy * weights.hand_synergy
            + num_buildable_nodes * weights.buildable_nodes * endgame_multiplier
            + num_tiles * weights.num_tiles * endgame_multiplier
            + num_in_hand * weights.hand_resources
            + discard_penalty
            + longest_road_length * longest_road_factor
            + (knights_played as f64) * weights.army_size
            + port_trade_value * weights.port_trade * endgame_multiplier
            + devs_bought * weights.devs_bought
            + devs_in_hand * weights.devs_in_hand_penalty
            + endgame_gap_term;

        ValueComponents {
            production,
            enemy_production,
            reachable_0: sum_resources(&reachable_at_zero),
            reachable_1: sum_resources(&reachable_at_one),
            reachable_2: sum_resources(&reachable_at_two),
            reachable_3: sum_resources(&reachable_at_three),
            hand_synergy,
            num_buildable_nodes,
            num_tiles,
            num_in_hand,
            discard_penalty,
            longest_road_length,
            longest_road_factor,
            city_trade_gap,
            port_trade_value,
            devs_bought,
            devs_in_hand,
            vps,
            total,
            prod_by_res,
            reachable_zero_by_res: reachable_at_zero,
        }
    }
}

#[inline]
fn action_changes_enemy_production(kind: &ValueActionKind) -> bool {
    matches!(kind, ValueActionKind::MoveRobber { .. })
}

#[inline]
fn action_is_non_structural(kind: &ValueActionKind) -> bool {
    !matches!(
        kind,
        ValueActionKind::BuildSettlement(_)
            | ValueActionKind::BuildCity(_)
            | ValueActionKind::BuildRoad(_)
    )
}

#[inline]
fn action_reuses_spatial_cache(kind: &ValueActionKind) -> bool {
    action_is_non_structural(kind) && !action_changes_enemy_production(kind)
}

fn build_spatial_value_cache(
    board: &Board,
    state: &State,
    road_state: &RoadState,
    player: PlayerId,
) -> SpatialValueCache {
    let prod_by_res = production_by_resource(board, state, player, true);
    let production = value_production(&prod_by_res, true);
    let (reachability, num_buildable_nodes) = reachability_no_enemy(board, state, player);
    let (num_tiles, player_port_flags, has_three_to_one) =
        player_node_features(board, state, player);
    SpatialValueCache {
        prod_by_res,
        production,
        reachability,
        num_buildable_nodes,
        num_tiles,
        player_port_flags,
        has_three_to_one,
        longest_road_length: road_state.length_for_player(player) as f64,
    }
}

fn enemy_production_total(board: &Board, state: &State, player: PlayerId) -> f64 {
    let mut enemy_production = 0.0;
    for offset in 1..PLAYER_COUNT {
        let enemy = ((player as usize + offset) % PLAYER_COUNT) as PlayerId;
        let enemy_prod = production_by_resource(board, state, enemy, true);
        enemy_production += value_production(&enemy_prod, false);
    }
    enemy_production
}

pub fn generate_playable_actions(
    board: &Board,
    state: &State,
    player: PlayerId,
) -> Vec<ValueAction> {
    let mut actions = Vec::new();

    match state.current_prompt {
        ActionPrompt::BuildInitialSettlement => {
            for node in 0..NODE_COUNT {
                let node_id = node as NodeId;
                if rules::is_legal_initial_settlement(board, state, node_id) {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::BuildSettlement(node_id),
                    });
                }
            }
        }
        ActionPrompt::BuildInitialRoad => {
            let anchor = state.last_initial_settlement[player as usize];
            if anchor != INVALID_NODE {
                for edge in board.node_edges[anchor as usize] {
                    if edge == INVALID_EDGE {
                        continue;
                    }
                    if rules::is_legal_initial_road(board, state, player, edge, anchor) {
                        actions.push(ValueAction {
                            player,
                            kind: ValueActionKind::BuildRoad(edge),
                        });
                    }
                }
            }
        }
        ActionPrompt::Discard => {
            actions.push(ValueAction {
                player,
                kind: ValueActionKind::Discard(None),
            });
        }
        ActionPrompt::MoveRobber => {
            for tile in 0..TILE_COUNT {
                let tile_id = tile as TileId;
                if tile_id == state.robber_tile {
                    continue;
                }
                let mut victims = Vec::new();
                for other in 0..PLAYER_COUNT {
                    let other_id = other as PlayerId;
                    if other_id == player {
                        continue;
                    }
                    if player_has_building_on_tile(board, state, other_id, tile_id)
                        && player_resource_total(state, other_id) > 0
                    {
                        victims.push(other_id);
                    }
                }
                if victims.is_empty() {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::MoveRobber {
                            tile: tile_id,
                            victim: None,
                            resource: None,
                        },
                    });
                } else {
                    for victim in victims {
                        actions.push(ValueAction {
                            player,
                            kind: ValueActionKind::MoveRobber {
                                tile: tile_id,
                                victim: Some(victim),
                                resource: None,
                            },
                        });
                    }
                }
            }
        }
        ActionPrompt::DecideTrade => {
            actions.push(ValueAction {
                player,
                kind: ValueActionKind::RejectTrade,
            });
            if can_accept_trade(state, player) {
                actions.push(ValueAction {
                    player,
                    kind: ValueActionKind::AcceptTrade,
                });
            }
        }
        ActionPrompt::DecideAcceptees => {
            actions.push(ValueAction {
                player,
                kind: ValueActionKind::CancelTrade,
            });
            for (idx, accepted) in state.acceptees.iter().enumerate() {
                if *accepted {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::ConfirmTrade(idx as PlayerId),
                    });
                }
            }
        }
        ActionPrompt::PlayTurn => {
            if state.is_road_building {
                for edge in 0..EDGE_COUNT {
                    let edge_id = edge as EdgeId;
                    if is_legal_build_road_free(board, state, player, edge_id) {
                        actions.push(ValueAction {
                            player,
                            kind: ValueActionKind::BuildRoad(edge_id),
                        });
                    }
                }
                return actions;
            }

            if can_play_dev(state, player, DevCard::YearOfPlenty) {
                actions.extend(year_of_plenty_actions(state, player));
            }
            if can_play_dev(state, player, DevCard::Monopoly) {
                for resource in RESOURCE_ORDER {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::PlayMonopoly(resource),
                    });
                }
            }
            if can_play_dev(state, player, DevCard::Knight) {
                actions.push(ValueAction {
                    player,
                    kind: ValueActionKind::PlayKnight,
                });
            }
            if can_play_dev(state, player, DevCard::RoadBuilding) {
                if (0..EDGE_COUNT)
                    .map(|edge| edge as EdgeId)
                    .any(|edge| is_legal_build_road_free(board, state, player, edge))
                {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::PlayRoadBuilding,
                    });
                }
            }

            if !state.has_rolled[player as usize] {
                actions.push(ValueAction {
                    player,
                    kind: ValueActionKind::Roll,
                });
                return actions;
            }

            actions.push(ValueAction {
                player,
                kind: ValueActionKind::EndTurn,
            });

            for node in 0..NODE_COUNT {
                let node_id = node as NodeId;
                if rules::is_legal_build_city(board, state, player, node_id) {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::BuildCity(node_id),
                    });
                }
            }
            for node in 0..NODE_COUNT {
                let node_id = node as NodeId;
                if rules::is_legal_build_settlement(board, state, player, node_id) {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::BuildSettlement(node_id),
                    });
                }
            }
            for edge in 0..EDGE_COUNT {
                let edge_id = edge as EdgeId;
                if rules::is_legal_build_road(board, state, player, edge_id) {
                    actions.push(ValueAction {
                        player,
                        kind: ValueActionKind::BuildRoad(edge_id),
                    });
                }
            }

            if can_buy_dev_card(state, player) {
                actions.push(ValueAction {
                    player,
                    kind: ValueActionKind::BuyDevCard,
                });
            }

            actions.extend(maritime_trade_actions(board, state, player));
        }
    }

    actions
}

pub fn apply_value_action(
    board: &Board,
    state: &mut State,
    road_state: &mut RoadState,
    army_state: &mut ArmyState,
    action: &ValueAction,
    rng: &mut impl RngCore,
) {
    match action.kind {
        ValueActionKind::BuildSettlement(node) => {
            if state.is_initial_build_phase
                && state.current_prompt == ActionPrompt::BuildInitialSettlement
            {
                apply_initial_settlement(board, state, road_state, node);
            } else {
                apply_build_settlement(board, state, road_state, action.player, node);
            }
        }
        ValueActionKind::BuildRoad(edge) => {
            if state.is_initial_build_phase
                && state.current_prompt == ActionPrompt::BuildInitialRoad
            {
                apply_initial_road(board, state, road_state, edge);
            } else if state.is_road_building {
                apply_build_road(board, state, road_state, action.player, edge, true);
                state.free_roads_available = state.free_roads_available.saturating_sub(1);
                if state.free_roads_available == 0 || !has_free_road(board, state, action.player) {
                    state.is_road_building = false;
                    state.free_roads_available = 0;
                }
            } else {
                apply_build_road(board, state, road_state, action.player, edge, false);
            }
        }
        ValueActionKind::BuildCity(node) => {
            apply_build_city(board, state, action.player, node);
        }
        ValueActionKind::Roll => {
            let roll = (roll_die(rng) as u32, roll_die(rng) as u32);
            apply_roll(board, state, roll);
        }
        ValueActionKind::EndTurn => {
            apply_end_turn(state, action.player);
        }
        ValueActionKind::Discard(counts) => {
            let discard = counts.unwrap_or_else(|| {
                random_discard_counts(&state.player_resources[action.player as usize], rng)
            });
            apply_discard(state, action.player, &discard);
        }
        ValueActionKind::MoveRobber {
            tile,
            victim,
            resource,
        } => {
            let stolen = if victim.is_some() && resource.is_none() {
                draw_random_resource(rng, &state.player_resources[victim.unwrap() as usize])
            } else {
                resource
            };
            apply_move_robber(state, tile, victim, stolen);
        }
        ValueActionKind::PlayYearOfPlenty(first, second) => {
            apply_year_of_plenty(state, first, second);
        }
        ValueActionKind::PlayMonopoly(resource) => {
            apply_monopoly(state, resource);
        }
        ValueActionKind::PlayKnight => {
            apply_knight(state, army_state);
            state.current_prompt = ActionPrompt::MoveRobber;
            state.is_moving_robber = true;
        }
        ValueActionKind::PlayRoadBuilding => {
            apply_road_building(state);
            state.current_prompt = ActionPrompt::PlayTurn;
        }
        ValueActionKind::MaritimeTrade { offer, rate, ask } => {
            apply_maritime_trade(state, action.player, offer, rate, ask);
        }
        ValueActionKind::BuyDevCard => {
            let _ = buy_dev_card(state, action.player);
        }
        ValueActionKind::AcceptTrade => {
            apply_accept_trade(state, action.player);
        }
        ValueActionKind::RejectTrade => {
            apply_reject_trade(state, action.player);
        }
        ValueActionKind::ConfirmTrade(partner) => {
            apply_confirm_trade(state, partner);
        }
        ValueActionKind::CancelTrade => {
            apply_cancel_trade(state);
        }
    }
}

pub fn apply_value_action_kernel(
    board: &Board,
    state: &mut State,
    road_state: &mut RoadState,
    army_state: &mut ArmyState,
    action: &ValueAction,
    rng: &mut impl RngCore,
) {
    match action.kind {
        ValueActionKind::BuildSettlement(node) => {
            if state.is_initial_build_phase
                && state.current_prompt == ActionPrompt::BuildInitialSettlement
            {
                apply_initial_settlement_kernel(board, state, road_state, node);
            } else {
                apply_build_settlement_kernel(board, state, road_state, action.player, node);
            }
        }
        ValueActionKind::BuildRoad(edge) => {
            if state.is_initial_build_phase
                && state.current_prompt == ActionPrompt::BuildInitialRoad
            {
                apply_initial_road_kernel(board, state, road_state, edge);
            } else if state.is_road_building {
                apply_build_road_kernel(board, state, road_state, action.player, edge, true);
                state.free_roads_available = state.free_roads_available.saturating_sub(1);
                if state.free_roads_available == 0 || !has_free_road(board, state, action.player) {
                    state.is_road_building = false;
                    state.free_roads_available = 0;
                }
            } else {
                apply_build_road_kernel(board, state, road_state, action.player, edge, false);
            }
        }
        ValueActionKind::BuildCity(node) => {
            apply_build_city_kernel(board, state, action.player, node);
        }
        ValueActionKind::Roll => {
            let roll = (roll_die(rng) as u32, roll_die(rng) as u32);
            apply_roll_kernel(board, state, roll);
        }
        ValueActionKind::EndTurn => {
            apply_end_turn(state, action.player);
        }
        ValueActionKind::Discard(counts) => {
            let discard = counts.unwrap_or_else(|| {
                random_discard_counts(&state.player_resources[action.player as usize], rng)
            });
            apply_discard_kernel(state, action.player, &discard);
        }
        ValueActionKind::MoveRobber {
            tile,
            victim,
            resource,
        } => {
            let stolen = if victim.is_some() && resource.is_none() {
                draw_random_resource(rng, &state.player_resources[victim.unwrap() as usize])
            } else {
                resource
            };
            apply_move_robber_kernel(state, tile, victim, stolen);
        }
        ValueActionKind::PlayYearOfPlenty(first, second) => {
            apply_year_of_plenty_kernel(state, first, second);
        }
        ValueActionKind::PlayMonopoly(resource) => {
            apply_monopoly_kernel(state, resource);
        }
        ValueActionKind::PlayKnight => {
            apply_knight(state, army_state);
            state.current_prompt = ActionPrompt::MoveRobber;
            state.is_moving_robber = true;
        }
        ValueActionKind::PlayRoadBuilding => {
            apply_road_building(state);
            state.current_prompt = ActionPrompt::PlayTurn;
        }
        ValueActionKind::MaritimeTrade { offer, rate, ask } => {
            apply_maritime_trade_kernel(state, action.player, offer, rate, ask);
        }
        ValueActionKind::BuyDevCard => {
            let _ = buy_dev_card_kernel(state, action.player);
        }
        ValueActionKind::AcceptTrade => {
            apply_accept_trade(state, action.player);
        }
        ValueActionKind::RejectTrade => {
            apply_reject_trade(state, action.player);
        }
        ValueActionKind::ConfirmTrade(partner) => {
            apply_confirm_trade_kernel(state, partner);
        }
        ValueActionKind::CancelTrade => {
            apply_cancel_trade(state);
        }
    }
}

fn year_of_plenty_actions(state: &State, player: PlayerId) -> Vec<ValueAction> {
    let mut options: Vec<(Resource, Option<Resource>)> = Vec::new();
    let mut push_unique = |entry: (Resource, Option<Resource>)| {
        if !options.contains(&entry) {
            options.push(entry);
        }
    };
    for (i, first) in RESOURCE_ORDER.iter().enumerate() {
        for second in RESOURCE_ORDER.iter().skip(i) {
            let first_ok = state.bank_resources[first.as_index()] > 0;
            let second_ok = state.bank_resources[second.as_index()] > 0;
            if first == second {
                if state.bank_resources[first.as_index()] >= 2 {
                    push_unique((*first, Some(*second)));
                } else if first_ok {
                    push_unique((*first, None));
                }
            } else if first_ok && second_ok {
                push_unique((*first, Some(*second)));
            } else {
                if first_ok {
                    push_unique((*first, None));
                }
                if second_ok {
                    push_unique((*second, None));
                }
            }
        }
    }

    options
        .into_iter()
        .map(|(first, second)| ValueAction {
            player,
            kind: ValueActionKind::PlayYearOfPlenty(first, second),
        })
        .collect()
}

fn maritime_trade_actions(board: &Board, state: &State, player: PlayerId) -> Vec<ValueAction> {
    let mut actions = Vec::new();
    for offer in RESOURCE_ORDER {
        let rate = trade_rate(board, state, player, offer);
        if state.player_resources[player as usize][offer.as_index()] < rate {
            continue;
        }
        for ask in RESOURCE_ORDER {
            if ask == offer {
                continue;
            }
            if state.bank_resources[ask.as_index()] > 0 {
                actions.push(ValueAction {
                    player,
                    kind: ValueActionKind::MaritimeTrade { offer, rate, ask },
                });
            }
        }
    }
    actions
}

fn has_free_road(board: &Board, state: &State, player: PlayerId) -> bool {
    (0..EDGE_COUNT).any(|edge| is_legal_build_road_free(board, state, player, edge as EdgeId))
}

fn compare_value_actions(a: &ValueAction, b: &ValueAction) -> Ordering {
    let kind_cmp = action_kind_sort_rank(&a.kind).cmp(&action_kind_sort_rank(&b.kind));
    if kind_cmp != Ordering::Equal {
        return kind_cmp;
    }
    compare_action_payload(&a.kind, &b.kind)
}

fn compare_action_payload(a: &ValueActionKind, b: &ValueActionKind) -> Ordering {
    match (a, b) {
        (ValueActionKind::BuildSettlement(node_a), ValueActionKind::BuildSettlement(node_b))
        | (ValueActionKind::BuildCity(node_a), ValueActionKind::BuildCity(node_b)) => {
            node_a.cmp(node_b)
        }
        (ValueActionKind::BuildRoad(edge_a), ValueActionKind::BuildRoad(edge_b)) => {
            let a_nodes = canonical_edge_nodes(*edge_a);
            let b_nodes = canonical_edge_nodes(*edge_b);
            a_nodes.cmp(&b_nodes)
        }
        (ValueActionKind::Discard(counts_a), ValueActionKind::Discard(counts_b)) => {
            match (counts_a, counts_b) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
                (Some(values_a), Some(values_b)) => values_a.cmp(values_b),
            }
        }
        (
            ValueActionKind::MoveRobber {
                tile: tile_a,
                victim: victim_a,
                resource: resource_a,
            },
            ValueActionKind::MoveRobber {
                tile: tile_b,
                victim: victim_b,
                resource: resource_b,
            },
        ) => {
            let tile_cmp = tile_coords(*tile_a).cmp(&tile_coords(*tile_b));
            if tile_cmp != Ordering::Equal {
                return tile_cmp;
            }
            let victim_cmp = compare_option_player(*victim_a, *victim_b);
            if victim_cmp != Ordering::Equal {
                return victim_cmp;
            }
            compare_option_resource(*resource_a, *resource_b)
        }
        (
            ValueActionKind::PlayYearOfPlenty(first_a, second_a),
            ValueActionKind::PlayYearOfPlenty(first_b, second_b),
        ) => {
            let first_cmp = resource_sort_rank(*first_a).cmp(&resource_sort_rank(*first_b));
            if first_cmp != Ordering::Equal {
                return first_cmp;
            }
            compare_option_resource(*second_a, *second_b)
        }
        (ValueActionKind::PlayMonopoly(resource_a), ValueActionKind::PlayMonopoly(resource_b)) => {
            resource_sort_rank(*resource_a).cmp(&resource_sort_rank(*resource_b))
        }
        (
            ValueActionKind::MaritimeTrade {
                offer: offer_a,
                rate: rate_a,
                ask: ask_a,
            },
            ValueActionKind::MaritimeTrade {
                offer: offer_b,
                rate: rate_b,
                ask: ask_b,
            },
        ) => {
            let offer_cmp = resource_sort_rank(*offer_a).cmp(&resource_sort_rank(*offer_b));
            if offer_cmp != Ordering::Equal {
                return offer_cmp;
            }
            let rate_cmp = rate_a.cmp(rate_b);
            if rate_cmp != Ordering::Equal {
                return rate_cmp;
            }
            resource_sort_rank(*ask_a).cmp(&resource_sort_rank(*ask_b))
        }
        (ValueActionKind::ConfirmTrade(partner_a), ValueActionKind::ConfirmTrade(partner_b)) => {
            color_sort_rank(*partner_a).cmp(&color_sort_rank(*partner_b))
        }
        _ => Ordering::Equal,
    }
}

fn canonical_edge_nodes(edge: EdgeId) -> (NodeId, NodeId) {
    let nodes = EDGE_NODES[edge as usize];
    if nodes[0] <= nodes[1] {
        (nodes[0], nodes[1])
    } else {
        (nodes[1], nodes[0])
    }
}

fn compare_option_player(a: Option<PlayerId>, b: Option<PlayerId>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(a), Some(b)) => color_sort_rank(a).cmp(&color_sort_rank(b)),
    }
}

fn compare_option_resource(a: Option<Resource>, b: Option<Resource>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(a), Some(b)) => resource_sort_rank(a).cmp(&resource_sort_rank(b)),
    }
}

fn action_kind_sort_rank(kind: &ValueActionKind) -> u8 {
    match kind {
        ValueActionKind::AcceptTrade => 0,
        ValueActionKind::BuildCity(_) => 1,
        ValueActionKind::BuildRoad(_) => 2,
        ValueActionKind::BuildSettlement(_) => 3,
        ValueActionKind::BuyDevCard => 4,
        ValueActionKind::CancelTrade => 5,
        ValueActionKind::ConfirmTrade(_) => 6,
        ValueActionKind::Discard(_) => 7,
        ValueActionKind::EndTurn => 8,
        ValueActionKind::MaritimeTrade { .. } => 9,
        ValueActionKind::MoveRobber { .. } => 10,
        ValueActionKind::PlayKnight => 11,
        ValueActionKind::PlayMonopoly(_) => 12,
        ValueActionKind::PlayRoadBuilding => 13,
        ValueActionKind::PlayYearOfPlenty(_, _) => 14,
        ValueActionKind::RejectTrade => 15,
        ValueActionKind::Roll => 16,
    }
}

fn resource_sort_rank(resource: Resource) -> u8 {
    match resource {
        Resource::Brick => 0,
        Resource::Ore => 1,
        Resource::Wool => 2,
        Resource::Grain => 3,
        Resource::Lumber => 4,
    }
}

fn color_sort_rank(player: PlayerId) -> u8 {
    match player {
        1 => 0, // BLUE
        2 => 1, // ORANGE
        0 => 2, // RED
        3 => 4, // WHITE
        _ => 3, // UNKNOWN
    }
}

fn tile_coords(tile: TileId) -> (i8, i8, i8) {
    if (tile as usize) < TILE_COORDS.len() {
        TILE_COORDS[tile as usize]
    } else {
        (0, 0, 0)
    }
}

fn sum_resources(values: &[f64; RESOURCE_COUNT]) -> f64 {
    values.iter().sum()
}

fn production_by_resource(
    board: &Board,
    state: &State,
    player: PlayerId,
    consider_robber: bool,
) -> [f64; RESOURCE_COUNT] {
    let table = node_production_table(board);
    let mut output = [0.0; RESOURCE_COUNT];
    let mut robbed = [false; NODE_COUNT];

    if consider_robber {
        for node in board.tile_nodes[state.robber_tile as usize] {
            if node != INVALID_NODE {
                robbed[node as usize] = true;
            }
        }
    }

    for node in 0..NODE_COUNT {
        let node_id = node as NodeId;
        if state.node_owner[node] != player {
            continue;
        }
        if consider_robber && robbed[node] {
            continue;
        }
        let multiplier = match state.node_level[node] {
            BuildingLevel::Settlement => 1.0,
            BuildingLevel::City => 2.0,
            BuildingLevel::Empty => 0.0,
        };
        if multiplier == 0.0 {
            continue;
        }
        for idx in 0..RESOURCE_COUNT {
            output[idx] += multiplier * table[node_id as usize][idx];
        }
    }

    output
}

fn value_production(values: &[f64; RESOURCE_COUNT], include_variety: bool) -> f64 {
    let mut sum = 0.0;
    let mut variety = 0.0;
    let order = [
        Resource::Grain,
        Resource::Ore,
        Resource::Wool,
        Resource::Lumber,
        Resource::Brick,
    ];
    for resource in order {
        let value = values[resource.as_index()];
        sum += value;
        if value != 0.0 {
            variety += 1.0;
        }
    }
    if include_variety {
        sum + variety * TRANSLATE_VARIETY * PROBA_POINT
    } else {
        sum
    }
}

fn node_production_table(board: &Board) -> &[[f64; RESOURCE_COUNT]; NODE_COUNT] {
    NODE_PRODUCTION.get_or_init(|| {
        let mut table = [[0.0; RESOURCE_COUNT]; NODE_COUNT];
        for node in 0..NODE_COUNT {
            for tile in board.node_tiles[node] {
                if tile == INVALID_TILE {
                    continue;
                }
                let tile_idx = tile as usize;
                let resource = match board.tile_resources[tile_idx] {
                    Some(resource) => resource,
                    None => continue,
                };
                let number = match board.tile_numbers[tile_idx] {
                    Some(number) => number,
                    None => continue,
                };
                table[node][resource.as_index()] += number_probability(number);
            }
        }
        table
    })
}

fn number_probability(number: u8) -> f64 {
    if number as usize >= DICE_PROBAS.len() {
        return 0.0;
    }
    DICE_PROBAS[number as usize]
}

fn reachability_no_enemy(
    board: &Board,
    state: &State,
    p0: PlayerId,
) -> ([[f64; RESOURCE_COUNT]; 4], usize) {
    let board_buildable = global_buildable_nodes(board, state);
    let mut outputs = [[0.0; RESOURCE_COUNT]; 4];

    let owned_or_buildable = owned_or_buildable_nodes(state, p0, &board_buildable);
    let zero_nodes = player_zero_nodes(board, state, p0);
    let num_buildable_nodes = board_buildable
        .iter()
        .zip(zero_nodes.iter())
        .filter(|(buildable, zero)| **buildable && **zero)
        .count();
    let mut owned_zero = zero_nodes;
    for node in 0..NODE_COUNT {
        if state.node_owner[node] != p0 && state.node_owner[node] != NO_PLAYER {
            owned_zero[node] = false;
        }
    }
    let mut production = [0.0; RESOURCE_COUNT];
    accumulate_production(board, owned_or_buildable, owned_zero, &mut production);
    outputs[0] = production;

    let enemy_nodes = enemy_nodes_mask(state, p0);
    let enemy_roads = enemy_roads_mask(state, p0);
    let level_nodes = reachable_nodes_by_level(board, &zero_nodes, &enemy_nodes, &enemy_roads);

    for level_idx in 0..3 {
        let mut level_mask = level_nodes[level_idx];
        for node in 0..NODE_COUNT {
            if enemy_nodes[node] {
                level_mask[node] = false;
            }
        }
        let mut production = [0.0; RESOURCE_COUNT];
        accumulate_production(board, owned_or_buildable, level_mask, &mut production);
        outputs[level_idx + 1] = production;
    }

    (outputs, num_buildable_nodes)
}

fn owned_or_buildable_nodes(
    state: &State,
    player: PlayerId,
    buildable: &[bool; NODE_COUNT],
) -> [bool; NODE_COUNT] {
    let mut mask = [false; NODE_COUNT];
    for node in 0..NODE_COUNT {
        if buildable[node] || state.node_owner[node] == player {
            mask[node] = true;
        }
    }
    mask
}

fn global_buildable_nodes(board: &Board, state: &State) -> [bool; NODE_COUNT] {
    let mut mask = [false; NODE_COUNT];
    for node in 0..NODE_COUNT {
        if state.node_owner[node] != NO_PLAYER {
            continue;
        }
        if node_is_adjacent_occupied(board, state, node as NodeId) {
            continue;
        }
        mask[node] = true;
    }
    mask
}

fn node_is_adjacent_occupied(board: &Board, state: &State, node: NodeId) -> bool {
    for edge in board.node_edges[node as usize] {
        if edge == INVALID_EDGE {
            continue;
        }
        let nodes = board.edge_nodes[edge as usize];
        let other = if nodes[0] == node { nodes[1] } else { nodes[0] };
        if state.node_owner[other as usize] != NO_PLAYER {
            return true;
        }
    }
    false
}

fn player_zero_nodes(board: &Board, state: &State, player: PlayerId) -> [bool; NODE_COUNT] {
    const STACK_CAPACITY: usize = NODE_COUNT + EDGE_COUNT * 2;
    let mut visited = [false; NODE_COUNT];
    let mut stack = [0u8; STACK_CAPACITY];
    let mut stack_len = 0usize;

    for node in 0..NODE_COUNT {
        if state.node_owner[node] == player {
            debug_assert!(stack_len < STACK_CAPACITY);
            if stack_len < STACK_CAPACITY {
                stack[stack_len] = node as NodeId;
                stack_len += 1;
            }
        }
    }

    for edge in 0..EDGE_COUNT {
        if state.edge_owner[edge] == player {
            let nodes = board.edge_nodes[edge];
            debug_assert!(stack_len + 1 < STACK_CAPACITY);
            if stack_len + 1 < STACK_CAPACITY {
                stack[stack_len] = nodes[0];
                stack_len += 1;
                stack[stack_len] = nodes[1];
                stack_len += 1;
            }
        }
    }

    while stack_len > 0 {
        stack_len -= 1;
        let node = stack[stack_len];
        let idx = node as usize;
        if visited[idx] {
            continue;
        }
        visited[idx] = true;
        if state.node_owner[idx] != NO_PLAYER && state.node_owner[idx] != player {
            continue;
        }
        for edge in board.node_edges[idx] {
            if edge == INVALID_EDGE {
                continue;
            }
            if state.edge_owner[edge as usize] != player {
                continue;
            }
            let nodes = board.edge_nodes[edge as usize];
            let other = if nodes[0] == node { nodes[1] } else { nodes[0] };
            if !visited[other as usize] {
                debug_assert!(stack_len < STACK_CAPACITY);
                if stack_len < STACK_CAPACITY {
                    stack[stack_len] = other;
                    stack_len += 1;
                }
            }
        }
    }

    visited
}

fn enemy_nodes_mask(state: &State, player: PlayerId) -> [bool; NODE_COUNT] {
    let mut mask = [false; NODE_COUNT];
    for node in 0..NODE_COUNT {
        let owner = state.node_owner[node];
        if owner != NO_PLAYER && owner != player {
            mask[node] = true;
        }
    }
    mask
}

fn enemy_roads_mask(state: &State, player: PlayerId) -> [bool; EDGE_COUNT] {
    let mut mask = [false; EDGE_COUNT];
    for edge in 0..EDGE_COUNT {
        let owner = state.edge_owner[edge];
        if owner != NO_PLAYER && owner != player {
            mask[edge] = true;
        }
    }
    mask
}

fn reachable_nodes_by_level(
    board: &Board,
    zero_nodes: &[bool; NODE_COUNT],
    enemy_nodes: &[bool; NODE_COUNT],
    enemy_roads: &[bool; EDGE_COUNT],
) -> [[bool; NODE_COUNT]; 3] {
    let mut results = [[false; NODE_COUNT]; 3];
    let mut level_nodes = *zero_nodes;
    let mut last_layer = level_nodes;

    for output in &mut results {
        let mut next_nodes = level_nodes;
        for node in 0..NODE_COUNT {
            if !last_layer[node] {
                continue;
            }
            if enemy_nodes[node] {
                continue;
            }
            for edge in board.node_edges[node] {
                if edge == INVALID_EDGE {
                    continue;
                }
                if enemy_roads[edge as usize] {
                    continue;
                }
                let nodes = board.edge_nodes[edge as usize];
                let other = if nodes[0] == node as u8 {
                    nodes[1]
                } else {
                    nodes[0]
                };
                next_nodes[other as usize] = true;
            }
        }
        level_nodes = next_nodes;
        last_layer = level_nodes;
        *output = level_nodes;
    }

    results
}

fn accumulate_production(
    board: &Board,
    owned_or_buildable: [bool; NODE_COUNT],
    nodes_mask: [bool; NODE_COUNT],
    output: &mut [f64; RESOURCE_COUNT],
) {
    let table = node_production_table(board);
    for node in 0..NODE_COUNT {
        if !owned_or_buildable[node] || !nodes_mask[node] {
            continue;
        }
        for idx in 0..RESOURCE_COUNT {
            output[idx] += table[node][idx];
        }
    }
}

fn player_node_features(
    board: &Board,
    state: &State,
    player: PlayerId,
) -> (usize, [bool; RESOURCE_COUNT], bool) {
    let mut owned = [false; TILE_COUNT];
    let mut ports = [false; RESOURCE_COUNT];
    let mut has_three_to_one = false;

    for node in 0..NODE_COUNT {
        if state.node_owner[node] != player {
            continue;
        }
        for tile in board.node_tiles[node] {
            if tile == INVALID_TILE {
                continue;
            }
            owned[tile as usize] = true;
        }
        match board.node_ports[node] {
            PortType::None => {}
            PortType::ThreeToOne => has_three_to_one = true,
            PortType::Brick => ports[Resource::Brick.as_index()] = true,
            PortType::Lumber => ports[Resource::Lumber.as_index()] = true,
            PortType::Ore => ports[Resource::Ore.as_index()] = true,
            PortType::Grain => ports[Resource::Grain.as_index()] = true,
            PortType::Wool => ports[Resource::Wool.as_index()] = true,
        }
    }
    let num_tiles = owned.iter().filter(|value| **value).count();
    (num_tiles, ports, has_three_to_one)
}

fn public_vps(
    state: &State,
    road_state: &RoadState,
    army_state: &ArmyState,
    player: PlayerId,
) -> u8 {
    let mut points = 0u8;
    for (idx, owner) in state.node_owner.iter().enumerate() {
        if *owner != player {
            continue;
        }
        points += match state.node_level[idx] {
            BuildingLevel::Settlement => 1,
            BuildingLevel::City => 2,
            BuildingLevel::Empty => 0,
        };
    }
    if road_state.owner() == Some(player) {
        points += 2;
    }
    if army_state.owner() == Some(player) {
        points += 2;
    }
    points
}

fn leader_by_public_vps(
    state: &State,
    road_state: &RoadState,
    army_state: &ArmyState,
    player: PlayerId,
) -> PlayerId {
    let mut best_player = None;
    let mut best_score = f64::MIN;
    for other in 0..PLAYER_COUNT {
        let other_id = other as u8;
        if other_id == player {
            continue;
        }
        let score = estimated_public_vps(state, road_state, army_state, other_id);
        if best_player.is_none() || score > best_score {
            best_player = Some(other_id);
            best_score = score;
        }
    }
    if let Some(best) = best_player {
        best
    } else {
        if player == 0 {
            1
        } else {
            0
        }
    }
}

fn estimated_public_vps(
    state: &State,
    road_state: &RoadState,
    army_state: &ArmyState,
    player: PlayerId,
) -> f64 {
    let base = public_vps(state, road_state, army_state, player) as f64;
    let devs_in_hand: u8 = state.dev_cards_in_hand[player as usize]
        .iter()
        .copied()
        .sum();
    base + (devs_in_hand as f64 / 3.0)
}

fn leader_robber_action(
    board: &Board,
    state: &State,
    road_state: &RoadState,
    army_state: &ArmyState,
    player: PlayerId,
    actions: &[ValueAction],
) -> Option<ValueAction> {
    let leader = leader_by_public_vps(state, road_state, army_state, player);
    let tile = best_leader_robber_tile(board, state, leader, player, state.robber_tile)?;
    if let Some(action) = actions.iter().find(|action| {
        matches!(
            action.kind,
            ValueActionKind::MoveRobber {
                tile: t,
                victim: Some(v),
                resource: None
            } if t == tile && v == leader
        )
    }) {
        return Some(action.clone());
    }
    let victim = best_victim_on_tile(board, state, road_state, army_state, player, tile)?;
    actions
        .iter()
        .find(|action| {
            matches!(
                action.kind,
                ValueActionKind::MoveRobber {
                    tile: t,
                    victim: Some(v),
                    resource: None
                } if t == tile && v == victim
            )
        })
        .cloned()
}

fn best_leader_robber_tile(
    board: &Board,
    state: &State,
    leader: PlayerId,
    player: PlayerId,
    current: TileId,
) -> Option<TileId> {
    let mut best_tile = None;
    let mut best_score = 0.0_f64;
    for tile_id in 0..TILE_COUNT {
        let tile = tile_id as TileId;
        if tile == current {
            continue;
        }
        if player_has_building_on_tile(board, state, player, tile) {
            continue;
        }
        let score = leader_tile_score(board, state, leader, tile);
        if score <= 0.0 {
            continue;
        }
        if best_tile.is_none() || score > best_score {
            best_tile = Some(tile);
            best_score = score;
        }
    }
    best_tile
}

fn leader_tile_score(board: &Board, state: &State, leader: PlayerId, tile: TileId) -> f64 {
    let number = match board.tile_numbers[tile as usize] {
        Some(number) => number,
        None => return 0.0,
    };
    let base = number_probability(number);
    if base <= 0.0 {
        return 0.0;
    }
    let mut multiplier = 0.0;
    for node in board.tile_nodes[tile as usize] {
        if node == INVALID_NODE {
            continue;
        }
        if state.node_owner[node as usize] != leader {
            continue;
        }
        multiplier += match state.node_level[node as usize] {
            BuildingLevel::Settlement => 1.0,
            BuildingLevel::City => 2.0,
            BuildingLevel::Empty => 0.0,
        };
    }
    base * multiplier
}

fn best_victim_on_tile(
    board: &Board,
    state: &State,
    road_state: &RoadState,
    army_state: &ArmyState,
    player: PlayerId,
    tile: TileId,
) -> Option<PlayerId> {
    let mut best = None;
    let mut best_score = f64::MIN;
    for other in 0..PLAYER_COUNT {
        let other_id = other as u8;
        if other_id == player {
            continue;
        }
        if !player_has_building_on_tile(board, state, other_id, tile) {
            continue;
        }
        if player_resource_total(state, other_id) == 0 {
            continue;
        }
        let score = estimated_public_vps(state, road_state, army_state, other_id);
        if best.is_none() || score > best_score {
            best = Some(other_id);
            best_score = score;
        }
    }
    best
}

fn random_discard_counts(
    hand: &[u8; RESOURCE_COUNT],
    rng: &mut impl RngCore,
) -> [u8; RESOURCE_COUNT] {
    let total: usize = hand.iter().map(|count| *count as usize).sum();
    let to_discard = total / 2;
    if to_discard == 0 {
        return [0u8; RESOURCE_COUNT];
    }

    let mut deck = Vec::with_capacity(total);
    for resource in PYTHON_RESOURCE_ORDER {
        let count = hand[resource.as_index()] as usize;
        for _ in 0..count {
            deck.push(resource);
        }
    }

    shuffle_with_rng(&mut deck, rng);

    let mut counts = [0u8; RESOURCE_COUNT];
    for resource in deck.iter().take(to_discard) {
        counts[resource.as_index()] += 1;
    }
    counts
}

fn draw_random_resource(rng: &mut impl RngCore, counts: &[u8; RESOURCE_COUNT]) -> Option<Resource> {
    let total: u64 = counts.iter().map(|count| *count as u64).sum();
    if total == 0 {
        return None;
    }
    let mut roll = next_u64_mod(rng, total);
    for resource in PYTHON_RESOURCE_ORDER {
        let amount = counts[resource.as_index()] as u64;
        if roll < amount {
            return Some(resource);
        }
        roll -= amount;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::STANDARD_BOARD;
    use crate::rng::rng_for_stream;

    fn road_and_army_equal(
        a_road: &RoadState,
        a_army: &ArmyState,
        b_road: &RoadState,
        b_army: &ArmyState,
    ) -> bool {
        for player in 0..PLAYER_COUNT {
            if a_road.length_for_player(player as PlayerId)
                != b_road.length_for_player(player as PlayerId)
            {
                return false;
            }
        }
        a_road.owner() == b_road.owner()
            && a_road.length() == b_road.length()
            && a_army.owner() == b_army.owner()
            && a_army.size() == b_army.size()
    }

    fn pick_action(
        actions: &[ValueAction],
        predicate: impl Fn(&ValueActionKind) -> bool,
    ) -> ValueAction {
        actions
            .iter()
            .find(|action| predicate(&action.kind))
            .cloned()
            .expect("expected action for scenario")
    }

    fn setup_initial_settlement_scenario() -> (State, RoadState, ArmyState, ValueAction) {
        let board = STANDARD_BOARD;
        let state = State::new();
        let action = pick_action(
            &generate_playable_actions(&board, &state, state.active_player),
            |kind| matches!(kind, ValueActionKind::BuildSettlement(_)),
        );
        (state, RoadState::empty(), ArmyState::empty(), action)
    }

    fn setup_initial_road_scenario() -> (State, RoadState, ArmyState, ValueAction) {
        let board = STANDARD_BOARD;
        let mut state = State::new();
        let mut road_state = RoadState::empty();
        let mut army_state = ArmyState::empty();
        let settlement = pick_action(
            &generate_playable_actions(&board, &state, state.active_player),
            |kind| matches!(kind, ValueActionKind::BuildSettlement(_)),
        );
        apply_value_action_kernel(
            &board,
            &mut state,
            &mut road_state,
            &mut army_state,
            &settlement,
            &mut rng_for_stream(11, 0),
        );
        let action = pick_action(
            &generate_playable_actions(&board, &state, state.active_player),
            |kind| matches!(kind, ValueActionKind::BuildRoad(_)),
        );
        (state, road_state, army_state, action)
    }

    fn setup_discard_scenario() -> (State, RoadState, ArmyState, ValueAction) {
        let board = STANDARD_BOARD;
        let mut state = State::new();
        state.is_initial_build_phase = false;
        state.current_prompt = ActionPrompt::Discard;
        state.turn_player = 0;
        state.active_player = 0;
        state.is_discarding = true;
        state.player_resources[0] = [2; RESOURCE_COUNT];
        let action = pick_action(&generate_playable_actions(&board, &state, 0), |kind| {
            matches!(kind, ValueActionKind::Discard(_))
        });
        (state, RoadState::empty(), ArmyState::empty(), action)
    }

    fn setup_move_robber_scenario() -> (State, RoadState, ArmyState, ValueAction) {
        let board = STANDARD_BOARD;
        let mut state = State::new();
        state.is_initial_build_phase = false;
        state.current_prompt = ActionPrompt::MoveRobber;
        state.turn_player = 0;
        state.active_player = 0;
        state.is_moving_robber = true;

        let mut target_tile = None;
        let mut target_node = None;
        for tile in 0..board.tile_nodes.len() {
            let tile_id = tile as u8;
            if tile_id == state.robber_tile {
                continue;
            }
            for node in board.tile_nodes[tile] {
                if node == INVALID_NODE {
                    continue;
                }
                target_tile = Some(tile_id);
                target_node = Some(node);
                break;
            }
            if target_tile.is_some() {
                break;
            }
        }
        let tile = target_tile.expect("expected tile");
        let node = target_node.expect("expected node");
        state.node_owner[node as usize] = 1;
        state.node_level[node as usize] = BuildingLevel::Settlement;
        state.player_resources[1][Resource::Brick.as_index()] = 2;

        let action = pick_action(&generate_playable_actions(&board, &state, 0), |kind| {
            matches!(
                kind,
                ValueActionKind::MoveRobber {
                    tile: action_tile,
                    victim: Some(1),
                    ..
                } if *action_tile == tile
            )
        });
        (state, RoadState::empty(), ArmyState::empty(), action)
    }

    fn setup_decide_trade_scenario() -> (State, RoadState, ArmyState, ValueAction) {
        let board = STANDARD_BOARD;
        let mut state = State::new();
        state.is_initial_build_phase = false;
        state.current_prompt = ActionPrompt::DecideTrade;
        state.turn_player = 0;
        state.active_player = 1;
        state.trade_offering_player = 0;
        state.current_trade = [0; RESOURCE_COUNT * 2];
        state.current_trade[Resource::Brick.as_index()] = 1;
        state.current_trade[RESOURCE_COUNT + Resource::Grain.as_index()] = 1;
        state.is_resolving_trade = true;
        state.player_resources[1][Resource::Grain.as_index()] = 1;
        let action = pick_action(&generate_playable_actions(&board, &state, 1), |kind| {
            matches!(kind, ValueActionKind::AcceptTrade)
        });
        (state, RoadState::empty(), ArmyState::empty(), action)
    }

    fn setup_confirm_trade_scenario() -> (State, RoadState, ArmyState, ValueAction) {
        let board = STANDARD_BOARD;
        let mut state = State::new();
        state.is_initial_build_phase = false;
        state.current_prompt = ActionPrompt::DecideAcceptees;
        state.turn_player = 0;
        state.active_player = 0;
        state.trade_offering_player = 0;
        state.is_resolving_trade = true;
        state.current_trade = [0; RESOURCE_COUNT * 2];
        state.current_trade[Resource::Brick.as_index()] = 1;
        state.current_trade[RESOURCE_COUNT + Resource::Grain.as_index()] = 1;
        state.acceptees[1] = true;
        state.player_resources[0][Resource::Brick.as_index()] = 1;
        state.player_resources[1][Resource::Grain.as_index()] = 1;
        let action = pick_action(&generate_playable_actions(&board, &state, 0), |kind| {
            matches!(kind, ValueActionKind::ConfirmTrade(1))
        });
        (state, RoadState::empty(), ArmyState::empty(), action)
    }

    fn setup_roll_scenario() -> (State, RoadState, ArmyState, ValueAction) {
        let board = STANDARD_BOARD;
        let mut state = State::new();
        state.is_initial_build_phase = false;
        state.current_prompt = ActionPrompt::PlayTurn;
        state.turn_player = 0;
        state.active_player = 0;
        state.has_rolled[0] = false;
        let action = pick_action(&generate_playable_actions(&board, &state, 0), |kind| {
            matches!(kind, ValueActionKind::Roll)
        });
        (state, RoadState::empty(), ArmyState::empty(), action)
    }

    #[test]
    fn reversible_apply_undo_is_identity_for_prompt_classes() {
        let board = STANDARD_BOARD;
        let scenarios = [
            setup_initial_settlement_scenario(),
            setup_initial_road_scenario(),
            setup_discard_scenario(),
            setup_move_robber_scenario(),
            setup_decide_trade_scenario(),
            setup_confirm_trade_scenario(),
            setup_roll_scenario(),
        ];

        for (idx, (state, road_state, army_state, action)) in scenarios.into_iter().enumerate() {
            let mut reversible_state = state.clone();
            let mut reversible_road = road_state;
            let mut reversible_army = army_state;
            let mut rng = rng_for_stream(100 + idx as u64, 0);
            let before_state = reversible_state.clone();
            let before_road = reversible_road;
            let before_army = reversible_army;
            let mut decision_delta = DecisionDelta::new();

            apply_value_action_reversible(
                &board,
                &mut reversible_state,
                &mut reversible_road,
                &mut reversible_army,
                &action,
                &mut rng,
                &mut decision_delta,
            );
            undo_value_action_reversible(
                &mut reversible_state,
                &mut reversible_road,
                &mut reversible_army,
                &decision_delta,
            );

            assert_eq!(reversible_state, before_state);
            assert!(road_and_army_equal(
                &reversible_road,
                &reversible_army,
                &before_road,
                &before_army
            ));
        }
    }

    #[test]
    fn reversible_apply_matches_kernel_apply_and_rng_consumption() {
        let board = STANDARD_BOARD;
        let scenarios = [
            setup_initial_settlement_scenario(),
            setup_initial_road_scenario(),
            setup_discard_scenario(),
            setup_move_robber_scenario(),
            setup_decide_trade_scenario(),
            setup_confirm_trade_scenario(),
            setup_roll_scenario(),
        ];

        for (idx, (state, road_state, army_state, action)) in scenarios.into_iter().enumerate() {
            let mut reversible_state = state.clone();
            let mut reversible_road = road_state;
            let mut reversible_army = army_state;
            let mut kernel_state = state;
            let mut kernel_road = road_state;
            let mut kernel_army = army_state;
            let mut reversible_rng = rng_for_stream(1000 + idx as u64, 0);
            let mut kernel_rng = rng_for_stream(1000 + idx as u64, 0);
            let mut decision_delta = DecisionDelta::new();

            apply_value_action_reversible(
                &board,
                &mut reversible_state,
                &mut reversible_road,
                &mut reversible_army,
                &action,
                &mut reversible_rng,
                &mut decision_delta,
            );
            apply_value_action_kernel(
                &board,
                &mut kernel_state,
                &mut kernel_road,
                &mut kernel_army,
                &action,
                &mut kernel_rng,
            );

            assert_eq!(reversible_state, kernel_state);
            assert!(road_and_army_equal(
                &reversible_road,
                &reversible_army,
                &kernel_road,
                &kernel_army
            ));
            assert_eq!(reversible_rng.next_u64(), kernel_rng.next_u64());
        }
    }
}
