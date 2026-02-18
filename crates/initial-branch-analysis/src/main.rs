use fastcore::board_config::board_from_json;
use fastcore::engine::{ArmyState, RoadState};
use fastcore::rng::rng_for_stream;
use fastcore::state::State;
use fastcore::types::{
    ActionPrompt, BuildingLevel, EdgeId, NodeId, PlayerId, Resource, INVALID_TILE, PLAYER_COUNT,
    RESOURCE_COUNT,
};
use fastcore::value_player::{
    apply_value_action, apply_value_action_kernel, generate_playable_actions,
    FastValueFunctionPlayer, ValueAction, ValueActionKind,
};
#[cfg(feature = "stackelberg_pruning")]
use rand::Rng;
use serde_json::Value;
#[cfg(feature = "stackelberg_pruning")]
use std::collections::HashMap;
use std::env;
#[cfg(feature = "stackelberg_pruning")]
use std::f64::consts::PI;
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_STATE_PATH: &str = "data/state_pre_last_settlement.json";
const DEFAULT_BOARD_PATH: &str = "data/board_example.json";
const DEFAULT_NUM_SIMS: u32 = 50;
const DEFAULT_SEED: u64 = 99;
const DEFAULT_START_SEED: u64 = 1000;
const DEFAULT_MAX_TURNS: u32 = 1000;
const DEFAULT_OUTPUT: &str = "data/analysis/initial_branch_analysis.csv";
const DEFAULT_ALL_SIMS_SUFFIX: &str = "_all_sims.csv";
const DEFAULT_BUDGET: u64 = 1000;
const DEFAULT_ALPHA0: f64 = 1.0;
const DEFAULT_BETA0: f64 = 1.0;
const DEFAULT_RHO: f64 = 0.5;
const DEFAULT_MIN_SAMPLES: u64 = 2;
const DEFAULT_BATCH_SIMS: usize = 0;
const MIN_FOLLOWER_SETTLEMENT_PIPS: u32 = 5;

#[cfg(feature = "stackelberg_pruning")]
const MIN_BETA_SHAPE: f64 = 1e-6;
#[cfg(feature = "stackelberg_pruning")]
const PROB_CLAMP: f64 = 1e-6;
#[cfg(feature = "stackelberg_pruning")]
const DEFAULT_RED_GLOBAL_PRIOR_WEIGHT: f64 = 0.001;
#[cfg(feature = "stackelberg_pruning")]
const RED_GLOBAL_PRIOR_TAU: f64 = 8.0;
#[cfg(feature = "stackelberg_pruning")]
const KAPPA: f64 = 2.0;
#[cfg(feature = "stackelberg_pruning")]
#[cfg(feature = "stackelberg_pruning")]
const USE_UNIFORM_STACKELBERG_BUDGET: bool = false;
#[cfg(feature = "stackelberg_pruning")]
const USE_GLOBAL_RED_POOLING: bool = true;

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Copy, Debug, Default)]
struct RedWinLoss {
    wins: f64,
    losses: f64,
}

#[cfg(feature = "stackelberg_pruning")]
type RedPooledCounts = HashMap<NodeId, RedWinLoss>;
#[cfg(not(feature = "stackelberg_pruning"))]
type RedPooledCounts = ();

#[derive(Clone, Debug)]
struct PlayoutResult {
    winner: Option<PlayerId>,
    vps_by_player: [u8; PLAYER_COUNT],
    num_turns: u32,
}

#[derive(Clone, Debug, Default)]
struct PlayoutAggregate {
    wins: [u64; PLAYER_COUNT],
    total_non_none: u64,
    total_vps: [u64; PLAYER_COUNT],
    total_turns: u64,
    samples: u64,
}

impl PlayoutAggregate {
    fn update(&mut self, result: &PlayoutResult) {
        self.samples += 1;
        if let Some(winner) = result.winner {
            self.wins[winner as usize] += 1;
            self.total_non_none += 1;
        }
        for idx in 0..PLAYER_COUNT {
            self.total_vps[idx] += result.vps_by_player[idx] as u64;
        }
        self.total_turns += result.num_turns as u64;
    }

    fn merge(&mut self, other: &Self) {
        self.total_non_none += other.total_non_none;
        self.total_turns += other.total_turns;
        self.samples += other.samples;
        for idx in 0..PLAYER_COUNT {
            self.wins[idx] += other.wins[idx];
            self.total_vps[idx] += other.total_vps[idx];
        }
    }
}

#[derive(Clone, Debug)]
struct PlayoutSummary {
    win_probabilities: [f64; PLAYER_COUNT],
    avg_vps_by_player: [f64; PLAYER_COUNT],
    avg_turns: f64,
    winner_label: String,
    workers_used: usize,
    wall_time_sec: f64,
    cpu_time_sec: f64,
}

#[derive(Clone, Debug)]
struct BranchEvaluation {
    score: f64,
    branch_index: usize,
    settlement_node: NodeId,
    road_edge: (NodeId, NodeId),
    settlement_node2: Option<NodeId>,
    road_edge2: Option<(NodeId, NodeId)>,
    pips: [u32; RESOURCE_COUNT],
    pips2: Option<[u32; RESOURCE_COUNT]>,
    summary: PlayoutSummary,
}

#[derive(Clone, Debug)]
struct BranchTask {
    branch_index: usize,
    player: PlayerId,
    settlement_node: NodeId,
    road_edge_id: EdgeId,
    road_edge_nodes: (NodeId, NodeId),
    pips: [u32; RESOURCE_COUNT],
}

#[derive(Clone, Debug)]
struct WhitePairTask {
    branch_index: usize,
    player: PlayerId,
    settlement_a: NodeId,
    settlement_b: NodeId,
    pips_a: [u32; RESOURCE_COUNT],
    pips_b: [u32; RESOURCE_COUNT],
}

#[derive(Clone, Debug)]
struct BranchResult {
    evaluation: BranchEvaluation,
    all_sims_entries: Vec<AllSimsEntry>,
    #[cfg(feature = "stackelberg_pruning")]
    red_pool_stats: RedPooledCounts,
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Debug)]
struct LeafStats {
    alpha_red: f64,
    beta_red: f64,
    alpha_blue: f64,
    beta_blue: f64,
    wins: [u64; PLAYER_COUNT],
    total_non_none: u64,
    total_vps: [u64; PLAYER_COUNT],
    total_turns: u64,
    samples: u64,
}

#[cfg(feature = "stackelberg_pruning")]
impl LeafStats {
    fn new(alpha0: f64, beta0: f64) -> Self {
        Self {
            alpha_red: alpha0,
            beta_red: beta0,
            alpha_blue: alpha0,
            beta_blue: beta0,
            wins: [0u64; PLAYER_COUNT],
            total_non_none: 0,
            total_vps: [0u64; PLAYER_COUNT],
            total_turns: 0,
            samples: 0,
        }
    }

    fn update(&mut self, result: &PlayoutResult, red: PlayerId, blue: Option<PlayerId>) {
        self.samples += 1;
        if let Some(win_id) = result.winner {
            self.wins[win_id as usize] += 1;
            self.total_non_none += 1;
        }
        for idx in 0..PLAYER_COUNT {
            self.total_vps[idx] += result.vps_by_player[idx] as u64;
        }
        self.total_turns += result.num_turns as u64;
        if result.winner == Some(red) {
            self.alpha_red += 1.0;
        } else {
            self.beta_red += 1.0;
        }
        if let Some(blue_id) = blue {
            if result.winner == Some(blue_id) {
                self.alpha_blue += 1.0;
            } else {
                self.beta_blue += 1.0;
            }
        }
    }
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Debug)]
struct RedBranch {
    action: PlacementAction,
    state: Arc<State>,
    road_state: RoadState,
    army_state: ArmyState,
    stats: LeafStats,
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Debug)]
struct RedSettlementGroup {
    roads: Vec<usize>,
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Debug)]
struct RedTree {
    branches: Vec<RedBranch>,
    settlements: Vec<RedSettlementGroup>,
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Debug)]
struct BlueBranch {
    action: PlacementAction,
    red_tree: RedTree,
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Debug)]
struct BlueSettlementGroup {
    roads: Vec<usize>,
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Debug)]
struct BlueTree {
    branches: Vec<BlueBranch>,
    settlements: Vec<BlueSettlementGroup>,
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Copy, Debug)]
enum OutcomeFlavor {
    Red,
    Blue,
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Debug)]
struct PairGroups {
    groups: Vec<Vec<(usize, usize)>>,
    index: Vec<Vec<usize>>,
}

#[cfg(feature = "stackelberg_pruning")]
fn build_pair_groups(tree: &BlueTree) -> PairGroups {
    let mut groups: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut index: Vec<Vec<usize>> = Vec::with_capacity(tree.branches.len());
    let mut map: HashMap<(NodeId, NodeId), usize> = HashMap::new();
    for (b_idx, branch) in tree.branches.iter().enumerate() {
        let leader_settlement = branch.action.settlement;
        let mut row = Vec::with_capacity(branch.red_tree.branches.len());
        for (r_idx, red_branch) in branch.red_tree.branches.iter().enumerate() {
            let follower_settlement = red_branch.action.settlement;
            let key = (leader_settlement, follower_settlement);
            let group_idx = *map.entry(key).or_insert_with(|| {
                groups.push(Vec::new());
                groups.len() - 1
            });
            groups[group_idx].push((b_idx, r_idx));
            row.push(group_idx);
        }
        index.push(row);
    }
    PairGroups { groups, index }
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Debug)]
struct PairParams {
    alpha0: f64,
    beta0: f64,
    kappa: f64,
    group_means: Vec<f64>,
}

#[cfg(feature = "stackelberg_pruning")]
impl PairParams {
    fn new(
        tree: &BlueTree,
        groups: &PairGroups,
        alpha0: f64,
        beta0: f64,
        flavor: OutcomeFlavor,
    ) -> Self {
        let group_means = pair_group_means(tree, groups, alpha0, beta0, flavor);
        Self {
            alpha0,
            beta0,
            kappa: KAPPA,
            group_means,
        }
    }
}

#[cfg(feature = "stackelberg_pruning")]
fn clamp_prob(p: f64) -> f64 {
    p.clamp(PROB_CLAMP, 1.0 - PROB_CLAMP)
}

#[cfg(feature = "stackelberg_pruning")]
fn road_counts(stats: &LeafStats, alpha0: f64, beta0: f64, flavor: OutcomeFlavor) -> (f64, f64) {
    let (alpha, beta) = match flavor {
        OutcomeFlavor::Red => (stats.alpha_red, stats.beta_red),
        OutcomeFlavor::Blue => (stats.alpha_blue, stats.beta_blue),
    };
    let wins = (alpha - alpha0).max(0.0);
    let losses = (beta - beta0).max(0.0);
    (wins, losses)
}

#[cfg(feature = "stackelberg_pruning")]
fn sample_plain_beta_from_counts<R: Rng>(
    rng: &mut R,
    wins: f64,
    losses: f64,
    alpha0: f64,
    beta0: f64,
) -> f64 {
    let alpha = (alpha0 + wins).max(MIN_BETA_SHAPE);
    let beta = (beta0 + losses).max(MIN_BETA_SHAPE);
    sample_beta(rng, alpha, beta)
}

#[cfg(feature = "stackelberg_pruning")]
fn settlement_counts(
    tree: &RedTree,
    settlement_idx: usize,
    alpha0: f64,
    beta0: f64,
    flavor: OutcomeFlavor,
) -> (f64, f64, u64) {
    let mut wins = 0.0;
    let mut losses = 0.0;
    let mut samples = 0u64;
    for &idx in &tree.settlements[settlement_idx].roads {
        let stats = &tree.branches[idx].stats;
        let (w, l) = road_counts(stats, alpha0, beta0, flavor);
        wins += w;
        losses += l;
        samples += stats.samples;
    }
    (wins, losses, samples)
}

#[cfg(feature = "stackelberg_pruning")]
fn blue_road_counts(tree: &BlueTree, blue_idx: usize, alpha0: f64, beta0: f64) -> (f64, f64, u64) {
    let red_tree = &tree.branches[blue_idx].red_tree;
    let mut wins = 0.0;
    let mut losses = 0.0;
    let mut samples = 0u64;
    for red_branch in &red_tree.branches {
        let stats = &red_branch.stats;
        let (w, l) = road_counts(stats, alpha0, beta0, OutcomeFlavor::Blue);
        wins += w;
        losses += l;
        samples += stats.samples;
    }
    (wins, losses, samples)
}

#[cfg(feature = "stackelberg_pruning")]
fn blue_settlement_counts(
    tree: &BlueTree,
    settlement_idx: usize,
    alpha0: f64,
    beta0: f64,
) -> (f64, f64, u64) {
    let mut wins = 0.0;
    let mut losses = 0.0;
    let mut samples = 0u64;
    for &blue_idx in &tree.settlements[settlement_idx].roads {
        let (w, l, s) = blue_road_counts(tree, blue_idx, alpha0, beta0);
        wins += w;
        losses += l;
        samples += s;
    }
    (wins, losses, samples)
}

#[cfg(feature = "stackelberg_pruning")]
fn group_red_settlements(tree: &BlueTree, groups: &PairGroups) -> Vec<NodeId> {
    let mut mapping = vec![0 as NodeId; groups.groups.len()];
    for (group_idx, members) in groups.groups.iter().enumerate() {
        if let Some(&(b_idx, r_idx)) = members.first() {
            mapping[group_idx] = tree.branches[b_idx].red_tree.branches[r_idx]
                .action
                .settlement;
        }
    }
    mapping
}

#[cfg(feature = "stackelberg_pruning")]
fn settlement_means(tree: &RedTree, alpha0: f64, beta0: f64, flavor: OutcomeFlavor) -> Vec<f64> {
    let prior_mean = alpha0 / (alpha0 + beta0);
    let mut means = Vec::with_capacity(tree.settlements.len());
    for group in &tree.settlements {
        let mut wins = 0.0;
        let mut losses = 0.0;
        for &idx in &group.roads {
            let (w, l) = road_counts(&tree.branches[idx].stats, alpha0, beta0, flavor);
            wins += w;
            losses += l;
        }
        let denom = wins + losses;
        let mean = if denom > 0.0 {
            (alpha0 + wins) / (alpha0 + beta0 + denom)
        } else {
            prior_mean
        };
        means.push(clamp_prob(mean));
    }
    means
}

#[cfg(feature = "stackelberg_pruning")]
fn pair_group_means(
    tree: &BlueTree,
    groups: &PairGroups,
    alpha0: f64,
    beta0: f64,
    flavor: OutcomeFlavor,
) -> Vec<f64> {
    let prior_mean = alpha0 / (alpha0 + beta0);
    let mut means = Vec::with_capacity(groups.groups.len());
    for group in &groups.groups {
        let mut wins = 0.0;
        let mut losses = 0.0;
        for &(b_idx, r_idx) in group {
            let stats = &tree.branches[b_idx].red_tree.branches[r_idx].stats;
            let (w, l) = road_counts(stats, alpha0, beta0, flavor);
            wins += w;
            losses += l;
        }
        let denom = wins + losses;
        let mean = if denom > 0.0 {
            (alpha0 + wins) / (alpha0 + beta0 + denom)
        } else {
            prior_mean
        };
        means.push(clamp_prob(mean));
    }
    means
}

#[cfg(feature = "stackelberg_pruning")]
fn merge_red_pool_counts(target: &mut RedPooledCounts, delta: &RedPooledCounts) {
    for (&settlement, stats) in delta.iter() {
        let entry = target.entry(settlement).or_default();
        entry.wins += stats.wins;
        entry.losses += stats.losses;
    }
}

#[cfg(feature = "stackelberg_pruning")]
fn accumulate_red_stats_by_settlement(tree: &BlueTree, alpha0: f64, beta0: f64) -> RedPooledCounts {
    let mut totals: RedPooledCounts = HashMap::new();
    for blue_branch in &tree.branches {
        for red_branch in &blue_branch.red_tree.branches {
            let settlement = red_branch.action.settlement;
            let entry = totals.entry(settlement).or_default();
            let (wins, losses) = road_counts(&red_branch.stats, alpha0, beta0, OutcomeFlavor::Red);
            entry.wins += wins;
            entry.losses += losses;
        }
    }
    totals
}

#[cfg(feature = "stackelberg_pruning")]
fn red_global_settlement_means_with_pool(
    tree: &BlueTree,
    alpha0: f64,
    beta0: f64,
    pooled_counts: Option<&RedPooledCounts>,
) -> HashMap<NodeId, f64> {
    let prior_mean = alpha0 / (alpha0 + beta0);
    let mut totals: RedPooledCounts = pooled_counts.cloned().unwrap_or_default();
    let local = accumulate_red_stats_by_settlement(tree, alpha0, beta0);
    merge_red_pool_counts(&mut totals, &local);

    let mut means = HashMap::with_capacity(totals.len());
    for (settlement, stats) in totals {
        let denom = stats.wins + stats.losses;
        let mean = if denom > 0.0 {
            (alpha0 + stats.wins) / (alpha0 + beta0 + denom)
        } else {
            prior_mean
        };
        means.insert(settlement, clamp_prob(mean));
    }
    means
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Debug)]
struct WarmStartState {
    next_idx: usize,
}

#[cfg(feature = "stackelberg_pruning")]
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

#[cfg(feature = "stackelberg_pruning")]
fn stackelberg_seed(base_seed: u64, blue_idx: usize, red_idx: usize, t: u64) -> u64 {
    let mut x = base_seed ^ (blue_idx as u64).wrapping_mul(0x9E3779B97F4A7C15);
    x = x ^ (red_idx as u64).wrapping_mul(0xBF58476D1CE4E5B9);
    x = x ^ t.wrapping_mul(0x94D049BB133111EB);
    splitmix64(x)
}

#[cfg(feature = "stackelberg_pruning")]
fn sample_standard_normal<R: Rng>(rng: &mut R) -> f64 {
    let u1 = rng.gen::<f64>().max(1e-12);
    let u2 = rng.gen::<f64>();
    (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
}

#[cfg(feature = "stackelberg_pruning")]
fn sample_gamma<R: Rng>(rng: &mut R, shape: f64) -> f64 {
    if shape < 1.0 {
        let u = rng.gen::<f64>().max(1e-12);
        return sample_gamma(rng, shape + 1.0) * u.powf(1.0 / shape);
    }
    let d = shape - 1.0 / 3.0;
    let c = (1.0 / (3.0 * d)).sqrt();
    loop {
        let x = sample_standard_normal(rng);
        let v = (1.0 + c * x).powi(3);
        if v <= 0.0 {
            continue;
        }
        let u = rng.gen::<f64>();
        if u < 1.0 - 0.0331 * x.powi(4) {
            return d * v;
        }
        if u.ln() < 0.5 * x * x + d * (1.0 - v + v.ln()) {
            return d * v;
        }
    }
}

#[cfg(feature = "stackelberg_pruning")]
fn sample_beta<R: Rng>(rng: &mut R, alpha: f64, beta: f64) -> f64 {
    let x = sample_gamma(rng, alpha);
    let y = sample_gamma(rng, beta);
    x / (x + y)
}

#[cfg(feature = "stackelberg_pruning")]
fn beta_mean(alpha: f64, beta: f64) -> f64 {
    alpha / (alpha + beta)
}

#[cfg(feature = "stackelberg_pruning")]
fn top_two_indices(values: &[f64]) -> (usize, usize) {
    let mut best = 0usize;
    let mut second = if values.len() > 1 { 1 } else { 0 };
    for (idx, value) in values.iter().enumerate() {
        if *value > values[best] || (*value - values[best]).abs() < f64::EPSILON && idx < best {
            second = best;
            best = idx;
        } else if idx != best
            && (*value > values[second]
                || (*value - values[second]).abs() < f64::EPSILON && idx < second)
        {
            second = idx;
        }
    }
    (best, second)
}

#[cfg(feature = "stackelberg_pruning")]
fn choose_top_two<R: Rng>(first: usize, second: usize, rho: f64, rng: &mut R) -> usize {
    if first == second {
        first
    } else if rng.gen::<f64>() < rho {
        second
    } else {
        first
    }
}

#[cfg(feature = "stackelberg_pruning")]
fn red_settlement_samples(tree: &RedTree, settlement_idx: usize) -> u64 {
    let mut total = 0u64;
    for &idx in &tree.settlements[settlement_idx].roads {
        total += tree.branches[idx].stats.samples;
    }
    total
}

#[cfg(feature = "stackelberg_pruning")]
fn blue_settlement_samples(tree: &BlueTree, settlement_idx: usize) -> u64 {
    let mut total = 0u64;
    for &b_idx in &tree.settlements[settlement_idx].roads {
        let red_tree = &tree.branches[b_idx].red_tree;
        for red_branch in &red_tree.branches {
            total += red_branch.stats.samples;
        }
    }
    total
}

#[derive(Clone, Debug)]
struct PlacementAction {
    color: String,
    settlement: NodeId,
    road: (NodeId, NodeId),
}

#[derive(Clone, Debug)]
struct AllSimsEntry {
    leader_branch_index: usize,
    leader: PlacementAction,
    leader_second: Option<PlacementAction>,
    followers: Vec<PlacementAction>,
    summary: PlayoutSummary,
    sims_run: u64,
    source: String,
}

#[derive(Clone, Debug)]
struct StackelbergConfig {
    budget: u64,
    alpha0: f64,
    beta0: f64,
    red_global_prior_weight: f64,
    rho: f64,
    min_samples: u64,
    batch_sims: usize,
}

#[cfg(not(feature = "stackelberg_pruning"))]
fn touch_stackelberg_config(config: &StackelbergConfig) {
    let _ = (
        config.budget,
        config.alpha0,
        config.beta0,
        config.red_global_prior_weight,
        config.rho,
        config.min_samples,
        config.batch_sims,
    );
}

#[derive(Clone, Debug)]
struct Args {
    state_path: String,
    board_path: String,
    num_sims: u32,
    workers: usize,
    seed: u64,
    start_seed: u64,
    sort_color: Option<String>,
    limit: Option<usize>,
    leader_settlement: Option<NodeId>,
    output: String,
    max_turns: u32,
    blue2: bool,
    orange2: bool,
    white12: bool,
    dry_run: bool,
    holdout_rerun: bool,
    holdout_only: bool,
    all_sims_output: Option<String>,
    budget: u64,
    alpha0: f64,
    beta0: f64,
    rho: f64,
    min_samples: u64,
    batch_sims: usize,
    red_global_prior_weight: f64,
}

fn parse_args() -> Args {
    let mut state_path = DEFAULT_STATE_PATH.to_string();
    let mut board_path = DEFAULT_BOARD_PATH.to_string();
    let mut num_sims = DEFAULT_NUM_SIMS;
    let mut workers = available_workers();
    let mut seed = DEFAULT_SEED;
    let mut start_seed = DEFAULT_START_SEED;
    let mut sort_color = None;
    let mut limit = None;
    let mut leader_settlement = None;
    let mut output = DEFAULT_OUTPUT.to_string();
    let mut max_turns = DEFAULT_MAX_TURNS;
    let mut blue2 = false;
    let mut orange2 = false;
    let mut white12 = false;
    let mut dry_run = false;
    let mut holdout_rerun = false;
    let mut holdout_only = false;
    let mut all_sims_output = None;
    let mut budget = DEFAULT_BUDGET;
    let mut alpha0 = DEFAULT_ALPHA0;
    let mut beta0 = DEFAULT_BETA0;
    let mut rho = DEFAULT_RHO;
    let mut min_samples = DEFAULT_MIN_SAMPLES;
    let mut batch_sims = DEFAULT_BATCH_SIMS;
    let mut red_global_prior_weight = DEFAULT_RED_GLOBAL_PRIOR_WEIGHT;

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
            "--num-sims" => {
                if let Some(value) = args.next() {
                    num_sims = value.parse().unwrap_or(DEFAULT_NUM_SIMS);
                }
            }
            "--holdout-sims" => {
                if let Some(value) = args.next() {
                    num_sims = value.parse().unwrap_or(DEFAULT_NUM_SIMS);
                }
            }
            "--workers" => {
                if let Some(value) = args.next() {
                    workers = value.parse().unwrap_or(workers);
                }
            }
            "--seed" => {
                if let Some(value) = args.next() {
                    seed = value.parse().unwrap_or(DEFAULT_SEED);
                }
            }
            "--start-seed" => {
                if let Some(value) = args.next() {
                    start_seed = value.parse().unwrap_or(DEFAULT_START_SEED);
                }
            }
            "--sort-color" => {
                if let Some(value) = args.next() {
                    sort_color = Some(value);
                }
            }
            "--limit" => {
                if let Some(value) = args.next() {
                    limit = value.parse().ok();
                }
            }
            "--leader-settlement" => {
                if let Some(value) = args.next() {
                    leader_settlement = value.parse::<u32>().ok().map(|v| v as NodeId);
                }
            }
            "--output" => {
                if let Some(value) = args.next() {
                    output = value;
                }
            }
            "--max-turns" => {
                if let Some(value) = args.next() {
                    max_turns = value.parse().unwrap_or(DEFAULT_MAX_TURNS);
                }
            }
            "--blue2" => {
                blue2 = true;
            }
            "--orange2" => {
                orange2 = true;
            }
            "--white12" => {
                white12 = true;
            }
            "--dry-run" => {
                dry_run = true;
            }
            "--holdout-rerun" => {
                holdout_rerun = true;
            }
            "--holdout-only" => {
                holdout_only = true;
            }
            "--all-sims-output" => {
                if let Some(value) = args.next() {
                    all_sims_output = Some(value);
                }
            }
            "--budget" => {
                if let Some(value) = args.next() {
                    budget = value.parse().unwrap_or(DEFAULT_BUDGET);
                }
            }
            "--alpha0" => {
                if let Some(value) = args.next() {
                    alpha0 = value.parse().unwrap_or(DEFAULT_ALPHA0);
                }
            }
            "--beta0" => {
                if let Some(value) = args.next() {
                    beta0 = value.parse().unwrap_or(DEFAULT_BETA0);
                }
            }
            "--rho" => {
                if let Some(value) = args.next() {
                    rho = value.parse().unwrap_or(DEFAULT_RHO);
                }
            }
            "--min-samples" => {
                if let Some(value) = args.next() {
                    min_samples = value.parse().unwrap_or(DEFAULT_MIN_SAMPLES);
                }
            }
            "--batch-sims" => {
                if let Some(value) = args.next() {
                    batch_sims = value.parse().unwrap_or(DEFAULT_BATCH_SIMS);
                }
            }
            "--red-global-prior-weight" => {
                if let Some(value) = args.next() {
                    red_global_prior_weight =
                        value.parse().unwrap_or(DEFAULT_RED_GLOBAL_PRIOR_WEIGHT);
                }
            }
            "-h" | "--help" => {
                eprintln!(
                    "Usage: initial-branch-analysis [--state <path>] [--board <path>] [--num-sims <count>] [--holdout-sims <count>] [--workers <count>] [--seed <seed>] [--start-seed <seed>] [--sort-color <COLOR>] [--limit <count>] [--leader-settlement <node>] [--output <path>] [--max-turns <turns>] [--blue2] [--orange2] [--white12] [--dry-run] [--holdout-rerun] [--holdout-only] [--all-sims-output <path>] [--budget <count>] [--alpha0 <a>] [--beta0 <b>] [--rho <p>] [--min-samples <count>] [--batch-sims <count>] [--red-global-prior-weight <w>] (default: infer stackelberg mode from state current_color)"
                );
                std::process::exit(0);
            }
            _ => {
                eprintln!("Unknown arg: {arg}");
                std::process::exit(2);
            }
        }
    }

    Args {
        state_path,
        board_path,
        num_sims,
        workers,
        seed,
        start_seed,
        sort_color,
        limit,
        leader_settlement,
        output,
        max_turns,
        blue2,
        orange2,
        white12,
        dry_run,
        holdout_rerun,
        holdout_only,
        all_sims_output,
        budget,
        alpha0,
        beta0,
        rho,
        min_samples,
        batch_sims,
        red_global_prior_weight,
    }
}

fn available_workers() -> usize {
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
}

#[cfg(feature = "parallel")]
fn init_global_pool(workers: usize) {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build_global();
    });
}

#[cfg(not(feature = "parallel"))]
fn init_global_pool(_workers: usize) {}

fn load_actions(path: &str) -> (Vec<(String, String, Value)>, Vec<String>, Option<String>) {
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

    let current_color = data["current_color"].as_str().map(|s| s.to_string());
    (actions, colors, current_color)
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

fn apply_action_history(
    board: &fastcore::board::Board,
    actions: &[(String, String, Value)],
    colors: &[String],
    seed: u64,
) -> (State, RoadState, ArmyState) {
    let mut rng = rng_for_stream(seed, 0);
    let mut state = State::new_with_rng_and_board(&mut rng, board);
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
                let node = payload.as_u64().expect("expected node id") as NodeId;
                ValueAction {
                    player,
                    kind: ValueActionKind::BuildSettlement(node),
                }
            }
            "BUILD_ROAD" => {
                let pair = payload.as_array().expect("expected node pair");
                let a = pair[0].as_u64().expect("expected node id") as NodeId;
                let b = pair[1].as_u64().expect("expected node id") as NodeId;
                let edge =
                    edge_id_for_nodes(board, a, b).expect("edge id not found for BUILD_ROAD");
                ValueAction {
                    player,
                    kind: ValueActionKind::BuildRoad(edge),
                }
            }
            other => panic!("unsupported action type: {other}"),
        };
        apply_value_action(
            board,
            &mut state,
            &mut road_state,
            &mut army_state,
            &value_action,
            &mut rng,
        );
    }

    (state, road_state, army_state)
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

fn truncate_before_color1(
    actions: &[(String, String, Value)],
    color: &str,
) -> Vec<(String, String, Value)> {
    if actions.len() < 2 {
        panic!("not enough actions to derive {}1 base state", color);
    }
    for idx in 0..actions.len() - 1 {
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
    actions.to_vec()
}

fn default_all_sims_output(output: &str) -> String {
    if let Some(stem) = output.strip_suffix(".csv") {
        format!("{stem}{DEFAULT_ALL_SIMS_SUFFIX}")
    } else {
        format!("{output}{DEFAULT_ALL_SIMS_SUFFIX}")
    }
}

fn split_all_sims_outputs(path: &str) -> (String, String) {
    if let Some(stem) = path.strip_suffix(".csv") {
        (format!("{stem}_ts.csv"), format!("{stem}_holdout.csv"))
    } else {
        (format!("{path}_ts.csv"), format!("{path}_holdout.csv"))
    }
}

fn ensure_parent_dir(path: &str) {
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).expect("failed to create output directory");
        }
    }
}

fn python_resource_index(resource: Resource) -> usize {
    match resource {
        Resource::Lumber => 0,
        Resource::Brick => 1,
        Resource::Wool => 2,
        Resource::Grain => 3,
        Resource::Ore => 4,
    }
}

fn pip_count(number: u8) -> u8 {
    match number {
        2 => 1,
        3 => 2,
        4 => 3,
        5 => 4,
        6 => 5,
        8 => 5,
        9 => 4,
        10 => 3,
        11 => 2,
        12 => 1,
        _ => 0,
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

fn color_resource_pips(
    board: &fastcore::board::Board,
    state: &State,
    player: PlayerId,
) -> [u32; RESOURCE_COUNT] {
    let mut pips = [0u32; RESOURCE_COUNT];
    for (node_idx, owner) in state.node_owner.iter().enumerate() {
        if *owner != player {
            continue;
        }
        let multiplier = match state.node_level[node_idx] {
            BuildingLevel::Settlement => 1,
            BuildingLevel::City => 2,
            BuildingLevel::Empty => 0,
        };
        if multiplier == 0 {
            continue;
        }
        let node_pips = settlement_pips(board, node_idx as NodeId);
        for (idx, value) in node_pips.iter().enumerate() {
            pips[idx] += (*value) * (multiplier as u32);
        }
    }
    pips
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
        points[player] +=
            state.dev_cards_in_hand[player][fastcore::types::DevCard::VictoryPoint.as_index()];
    }
    if let Some(owner) = road_state.owner() {
        points[owner as usize] += 2;
    }
    if let Some(owner) = army_state.owner() {
        points[owner as usize] += 2;
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

fn simulate_from_state_with_scratch(
    board: &fastcore::board::Board,
    base_state: &State,
    base_road: RoadState,
    base_army: ArmyState,
    seed: u64,
    max_turns: u32,
    scratch_state: &mut State,
    players: &[FastValueFunctionPlayer; PLAYER_COUNT],
) -> PlayoutResult {
    let mut rng = rng_for_stream(seed, 0);
    scratch_state.clone_from(base_state);
    let mut road_state = base_road;
    let mut army_state = base_army;
    let mut winner = None;

    loop {
        if scratch_state.num_turns >= max_turns {
            break;
        }
        let player = scratch_state.active_player as usize;
        let action =
            players[player].decide(board, scratch_state, &road_state, &army_state, &mut rng);
        apply_value_action_kernel(
            board,
            scratch_state,
            &mut road_state,
            &mut army_state,
            &action,
            &mut rng,
        );
        if let Some(found) = check_winner(scratch_state, &road_state, &army_state) {
            winner = Some(found);
            break;
        }
    }

    let winner = winner.or_else(|| check_winner(scratch_state, &road_state, &army_state));
    let vps_by_player = player_points(scratch_state, &road_state, &army_state);
    PlayoutResult {
        winner,
        vps_by_player,
        num_turns: scratch_state.num_turns,
    }
}

fn win_probabilities_from_counts(wins: &[u64; PLAYER_COUNT], total: u64) -> [f64; PLAYER_COUNT] {
    if total == 0 {
        return [0.0; PLAYER_COUNT];
    }
    let mut probs = [0.0; PLAYER_COUNT];
    for idx in 0..PLAYER_COUNT {
        probs[idx] = ((wins[idx] as f64 / total as f64) * 100.0 * 10.0).round() / 10.0;
    }
    probs
}

fn average_vps_from_totals(totals: &[u64; PLAYER_COUNT], sims_run: u64) -> [f64; PLAYER_COUNT] {
    if sims_run == 0 {
        return [0.0; PLAYER_COUNT];
    }
    let mut avg = [0.0; PLAYER_COUNT];
    let count = sims_run as f64;
    for idx in 0..PLAYER_COUNT {
        avg[idx] = (totals[idx] as f64 / count * 100.0).round() / 100.0;
    }
    avg
}

fn average_turns_from_total(total_turns: u64, sims_run: u64) -> f64 {
    if sims_run == 0 {
        return 0.0;
    }
    let avg = total_turns as f64 / sims_run as f64;
    (avg * 100.0).round() / 100.0
}

fn dominant_winner_label(colors: &[String], win_probs: &[f64; PLAYER_COUNT]) -> String {
    let max_prob = win_probs.iter().cloned().fold(0.0_f64, f64::max);
    let mut winners = Vec::new();
    for (idx, prob) in win_probs.iter().enumerate() {
        if (*prob - max_prob).abs() < f64::EPSILON {
            if let Some(color) = colors.get(idx) {
                winners.push(color.clone());
            }
        }
    }
    winners.sort();
    winners.join("|")
}

#[cfg(feature = "stackelberg_pruning")]
fn summary_from_leaf(stats: &LeafStats, colors: &[String], workers_used: usize) -> PlayoutSummary {
    let win_probs = win_probabilities_from_counts(&stats.wins, stats.total_non_none);
    let avg_vps = average_vps_from_totals(&stats.total_vps, stats.samples);
    let avg_turns = average_turns_from_total(stats.total_turns, stats.samples);
    let winner_label = dominant_winner_label(colors, &win_probs);
    PlayoutSummary {
        win_probabilities: win_probs,
        avg_vps_by_player: avg_vps,
        avg_turns,
        winner_label,
        workers_used,
        wall_time_sec: 0.0,
        cpu_time_sec: 0.0,
    }
}

fn dry_run_summary(colors: &[String]) -> PlayoutSummary {
    let mut win_probs = [0.0; PLAYER_COUNT];
    let uniform = 100.0 / PLAYER_COUNT as f64;
    for prob in &mut win_probs {
        *prob = uniform;
    }
    let winner_label = dominant_winner_label(colors, &win_probs);
    PlayoutSummary {
        win_probabilities: win_probs,
        avg_vps_by_player: [0.0; PLAYER_COUNT],
        avg_turns: 0.0,
        winner_label,
        workers_used: 0,
        wall_time_sec: 0.0,
        cpu_time_sec: 0.0,
    }
}

fn summarize_playouts(
    board: &fastcore::board::Board,
    state: &State,
    road_state: RoadState,
    army_state: ArmyState,
    seeds: &[u64],
    workers: usize,
    colors: &[String],
    max_turns: u32,
) -> PlayoutSummary {
    let worker_count = effective_workers(workers, seeds.len());
    let wall_start = Instant::now();

    let aggregate = run_playouts(
        board,
        state,
        road_state,
        army_state,
        seeds,
        worker_count,
        max_turns,
    );
    let wall_time = wall_start.elapsed().as_secs_f64();
    let cpu_time = wall_time;

    let win_probs = win_probabilities_from_counts(&aggregate.wins, aggregate.total_non_none);
    let avg_vps = average_vps_from_totals(&aggregate.total_vps, aggregate.samples);
    let avg_turns = average_turns_from_total(aggregate.total_turns, aggregate.samples);
    let winner_label = dominant_winner_label(colors, &win_probs);

    PlayoutSummary {
        win_probabilities: win_probs,
        avg_vps_by_player: avg_vps,
        avg_turns,
        winner_label,
        workers_used: worker_count,
        wall_time_sec: wall_time,
        cpu_time_sec: cpu_time,
    }
}

#[cfg_attr(feature = "stackelberg_pruning", allow(dead_code))]
fn summarize_with_followers(
    board: &fastcore::board::Board,
    state: &State,
    road_state: RoadState,
    army_state: ArmyState,
    followers: &[PlayerId],
    base_seed: u64,
    seeds: &[u64],
    workers: usize,
    colors: &[String],
    max_turns: u32,
    leader: &PlacementAction,
    leader_branch_index: usize,
    all_sims_entries: &mut Option<Vec<AllSimsEntry>>,
) -> PlayoutSummary {
    let mut prefix = Vec::new();
    summarize_followers_recursive(
        board,
        state,
        road_state,
        army_state,
        followers,
        base_seed,
        seeds,
        workers,
        colors,
        max_turns,
        leader,
        leader_branch_index,
        &mut prefix,
        all_sims_entries,
    )
}

#[cfg(feature = "stackelberg_pruning")]
fn summarize_with_followers_pruned(
    board: &fastcore::board::Board,
    state: &State,
    road_state: RoadState,
    army_state: ArmyState,
    followers: &[PlayerId],
    base_seed: u64,
    seeds: &[u64],
    workers: usize,
    colors: &[String],
    max_turns: u32,
    leader: &PlacementAction,
    leader_branch_index: usize,
    all_sims_entries: &mut Option<Vec<AllSimsEntry>>,
    include_ts_entries: bool,
    stackelberg: &StackelbergConfig,
    holdout_rerun: bool,
    pooled_red_counts: Option<&RedPooledCounts>,
    red_pool_out: Option<&mut RedPooledCounts>,
) -> PlayoutSummary {
    if followers.is_empty() {
        return summarize_playouts(
            board, state, road_state, army_state, seeds, workers, colors, max_turns,
        );
    }
    if followers.len() > 2 {
        let mut prefix = Vec::new();
        return summarize_followers_recursive(
            board,
            state,
            road_state,
            army_state,
            followers,
            base_seed,
            seeds,
            workers,
            colors,
            max_turns,
            leader,
            leader_branch_index,
            &mut prefix,
            all_sims_entries,
        );
    }

    let base_seed_l = base_seed ^ (leader_branch_index as u64).wrapping_mul(0x9E3779B97F4A7C15);
    let mut rng = rng_for_stream(base_seed_l, 1);

    if followers.len() == 1 {
        let responder = followers[0];
        let mut red_tree = build_red_tree(
            board,
            state,
            road_state,
            army_state,
            responder,
            base_seed,
            colors,
            stackelberg.alpha0,
            stackelberg.beta0,
        );
        if red_tree.branches.is_empty() {
            return summarize_playouts(
                board, state, road_state, army_state, seeds, workers, colors, max_turns,
            );
        }

        let mut remaining = stackelberg.budget;
        let mut warm = WarmStartState { next_idx: 0 };
        while remaining > 0 {
            let mut batch = stackelberg.batch_sims.max(1).min(remaining as usize);
            if batch == 1 && remaining > 1 {
                batch = 2;
            }
            let mut pending_leaves = vec![0u64; red_tree.branches.len()];
            let mut pending_settlements = vec![0u64; red_tree.settlements.len()];
            let mut tasks = Vec::with_capacity(batch);
            for _ in 0..batch {
                let red_idx = next_warm_red_settlement(
                    &red_tree,
                    &mut pending_settlements,
                    stackelberg.min_samples,
                    &mut warm,
                    &mut rng,
                    stackelberg.alpha0,
                    stackelberg.beta0,
                )
                .unwrap_or_else(|| {
                    select_red_arm_ttts(
                        &red_tree,
                        stackelberg.rho,
                        &mut rng,
                        stackelberg.alpha0,
                        stackelberg.beta0,
                    )
                });
                let t = red_tree.branches[red_idx].stats.samples + pending_leaves[red_idx];
                pending_leaves[red_idx] += 1;
                let seed = stackelberg_seed(base_seed_l, 0, red_idx, t);
                let branch = &red_tree.branches[red_idx];
                tasks.push(SimTask {
                    blue_idx: 0,
                    red_idx,
                    state: Arc::clone(&branch.state),
                    road_state: branch.road_state,
                    army_state: branch.army_state,
                    seed,
                });
            }
            let outcomes = run_sim_tasks(board, &tasks, max_turns);
            for outcome in outcomes {
                let branch = &mut red_tree.branches[outcome.red_idx];
                branch.stats.update(&outcome.result, responder, None);
            }
            remaining = remaining.saturating_sub(batch as u64);
        }

        if include_ts_entries {
            if let Some(entries) = all_sims_entries.as_mut() {
                for branch in &red_tree.branches {
                    if branch.stats.samples == 0 {
                        continue;
                    }
                    let summary = summary_from_leaf(&branch.stats, colors, workers);
                    entries.push(AllSimsEntry {
                        leader_branch_index,
                        leader: leader.clone(),
                        leader_second: None,
                        followers: vec![branch.action.clone()],
                        summary,
                        sims_run: branch.stats.samples,
                        source: "ts".to_string(),
                    });
                }
            }
        }

        let settlement_means = settlement_means(
            &red_tree,
            stackelberg.alpha0,
            stackelberg.beta0,
            OutcomeFlavor::Red,
        );
        let mut best_settlement_idx = 0usize;
        let mut best_score = -1.0;
        for s_idx in 0..red_tree.settlements.len() {
            let (wins, losses, _samples) = settlement_counts(
                &red_tree,
                s_idx,
                stackelberg.alpha0,
                stackelberg.beta0,
                OutcomeFlavor::Red,
            );
            let p_s = settlement_means[s_idx];
            let alpha = (KAPPA * p_s + wins).max(MIN_BETA_SHAPE);
            let beta = (KAPPA * (1.0 - p_s) + losses).max(MIN_BETA_SHAPE);
            let score = beta_mean(alpha, beta);
            if score > best_score
                || (score - best_score).abs() < f64::EPSILON && s_idx < best_settlement_idx
            {
                best_score = score;
                best_settlement_idx = s_idx;
            }
        }
        let roads = &red_tree.settlements[best_settlement_idx].roads;
        let mut best_idx = roads[0];
        let mut best_leaf_score = -1.0;
        for &idx in roads {
            let stats = &red_tree.branches[idx].stats;
            let (wins, losses) = road_counts(
                stats,
                stackelberg.alpha0,
                stackelberg.beta0,
                OutcomeFlavor::Red,
            );
            let alpha = (stackelberg.alpha0 + wins).max(MIN_BETA_SHAPE);
            let beta = (stackelberg.beta0 + losses).max(MIN_BETA_SHAPE);
            let score = beta_mean(alpha, beta);
            if score > best_leaf_score
                || (score - best_leaf_score).abs() < f64::EPSILON && idx < best_idx
            {
                best_leaf_score = score;
                best_idx = idx;
            }
        }

        let chosen = &red_tree.branches[best_idx];
        let (summary, holdout_sims_run) = if holdout_rerun {
            (
                summarize_playouts(
                    board,
                    chosen.state.as_ref(),
                    chosen.road_state,
                    chosen.army_state,
                    seeds,
                    workers,
                    colors,
                    max_turns,
                ),
                seeds.len() as u64,
            )
        } else {
            (
                summary_from_leaf(&chosen.stats, colors, workers),
                chosen.stats.samples,
            )
        };

        if let Some(entries) = all_sims_entries.as_mut() {
            entries.push(AllSimsEntry {
                leader_branch_index,
                leader: leader.clone(),
                leader_second: None,
                followers: vec![chosen.action.clone()],
                summary: summary.clone(),
                sims_run: holdout_sims_run,
                source: "holdout".to_string(),
            });
        }
        if let Some(out) = red_pool_out {
            out.clear();
        }

        return summary;
    }

    let blue = followers[0];
    let red = followers[1];
    let mut blue_tree = build_blue_tree(
        board,
        state,
        road_state,
        army_state,
        blue,
        red,
        base_seed,
        colors,
        stackelberg.alpha0,
        stackelberg.beta0,
    );
    if blue_tree.branches.is_empty() {
        return summarize_playouts(
            board, state, road_state, army_state, seeds, workers, colors, max_turns,
        );
    }

    let pair_groups = build_pair_groups(&blue_tree);
    let group_red_settlements = group_red_settlements(&blue_tree, &pair_groups);
    let (flat, flat_index) = build_flat_indices(&blue_tree);
    if USE_UNIFORM_STACKELBERG_BUDGET {
        let mut remaining = stackelberg.budget.max(1);
        let mut offset = 0usize;
        while remaining > 0 {
            let mut batch = stackelberg.batch_sims.max(1).min(remaining as usize);
            if batch == 1 && remaining > 1 {
                batch = 2;
            }
            let mut pending_leaves = vec![0u64; flat.len()];
            let mut tasks = Vec::with_capacity(batch);
            for i in 0..batch {
                let flat_idx = (offset + i) % flat.len();
                let (b_idx, r_idx) = flat[flat_idx];
                let t = blue_tree.branches[b_idx].red_tree.branches[r_idx]
                    .stats
                    .samples
                    + pending_leaves[flat_idx];
                pending_leaves[flat_idx] += 1;
                let seed = stackelberg_seed(base_seed_l, b_idx, r_idx, t);
                let branch = &blue_tree.branches[b_idx].red_tree.branches[r_idx];
                tasks.push(SimTask {
                    blue_idx: b_idx,
                    red_idx: r_idx,
                    state: Arc::clone(&branch.state),
                    road_state: branch.road_state,
                    army_state: branch.army_state,
                    seed,
                });
            }
            offset = (offset + batch) % flat.len();
            let outcomes = run_sim_tasks(board, &tasks, max_turns);
            for outcome in outcomes {
                let branch =
                    &mut blue_tree.branches[outcome.blue_idx].red_tree.branches[outcome.red_idx];
                branch.stats.update(&outcome.result, red, Some(blue));
            }
            remaining = remaining.saturating_sub(batch as u64);
        }
    } else {
        let mut warm = WarmStartState { next_idx: 0 };
        let mut remaining = stackelberg.budget.max(1);
        while remaining > 0 {
            let mut batch = stackelberg.batch_sims.max(1).min(remaining as usize);
            if batch == 1 && remaining > 1 {
                batch = 2;
            }
            let mut pending_leaves = vec![0u64; flat.len()];
            let mut pending_settlements = vec![0u64; blue_tree.settlements.len()];
            let global_red_means = red_global_settlement_means_with_pool(
                &blue_tree,
                stackelberg.alpha0,
                stackelberg.beta0,
                pooled_red_counts,
            );
            let mut tasks = Vec::with_capacity(batch);
            for _ in 0..batch {
                let (b_idx, r_idx) = next_warm_blue_settlement(
                    &blue_tree,
                    &pair_groups,
                    &group_red_settlements,
                    &global_red_means,
                    stackelberg.red_global_prior_weight,
                    &mut pending_settlements,
                    stackelberg.min_samples,
                    &mut warm,
                    &mut rng,
                    stackelberg.alpha0,
                    stackelberg.beta0,
                    stackelberg.rho,
                )
                .unwrap_or_else(|| {
                    select_blue_red_arm_ttts(
                        &blue_tree,
                        &pair_groups,
                        &group_red_settlements,
                        &global_red_means,
                        stackelberg.red_global_prior_weight,
                        stackelberg.rho,
                        &mut rng,
                        stackelberg.alpha0,
                        stackelberg.beta0,
                    )
                });
                let flat_idx = flat_index[b_idx][r_idx];
                let t = blue_tree.branches[b_idx].red_tree.branches[r_idx]
                    .stats
                    .samples
                    + pending_leaves[flat_idx];
                pending_leaves[flat_idx] += 1;
                let seed = stackelberg_seed(base_seed_l, b_idx, r_idx, t);
                let branch = &blue_tree.branches[b_idx].red_tree.branches[r_idx];
                tasks.push(SimTask {
                    blue_idx: b_idx,
                    red_idx: r_idx,
                    state: Arc::clone(&branch.state),
                    road_state: branch.road_state,
                    army_state: branch.army_state,
                    seed,
                });
            }
            let outcomes = run_sim_tasks(board, &tasks, max_turns);
            for outcome in outcomes {
                let branch =
                    &mut blue_tree.branches[outcome.blue_idx].red_tree.branches[outcome.red_idx];
                branch.stats.update(&outcome.result, red, Some(blue));
            }
            remaining = remaining.saturating_sub(batch as u64);
        }
    }

    if include_ts_entries {
        if let Some(entries) = all_sims_entries.as_mut() {
            for (_b_idx, blue_branch) in blue_tree.branches.iter().enumerate() {
                for (_r_idx, red_branch) in blue_branch.red_tree.branches.iter().enumerate() {
                    if red_branch.stats.samples == 0 {
                        continue;
                    }
                    let summary = summary_from_leaf(&red_branch.stats, colors, workers);
                    entries.push(AllSimsEntry {
                        leader_branch_index,
                        leader: leader.clone(),
                        leader_second: None,
                        followers: vec![blue_branch.action.clone(), red_branch.action.clone()],
                        summary,
                        sims_run: red_branch.stats.samples,
                        source: "ts".to_string(),
                    });
                }
            }
        }
    }

    let mut best_blue_settlement_idx = 0usize;
    let mut best_blue_score = -1.0;
    for s_idx in 0..blue_tree.settlements.len() {
        let (wins, losses, _samples) =
            blue_settlement_counts(&blue_tree, s_idx, stackelberg.alpha0, stackelberg.beta0);
        let alpha = (stackelberg.alpha0 + wins).max(MIN_BETA_SHAPE);
        let beta = (stackelberg.beta0 + losses).max(MIN_BETA_SHAPE);
        let score = beta_mean(alpha, beta);
        if score > best_blue_score
            || (score - best_blue_score).abs() < f64::EPSILON && s_idx < best_blue_settlement_idx
        {
            best_blue_score = score;
            best_blue_settlement_idx = s_idx;
        }
    }

    let blue_roads = &blue_tree.settlements[best_blue_settlement_idx].roads;
    let mut best_blue_idx = blue_roads[0];
    let mut best_blue_leaf_score = -1.0;
    for &b_idx in blue_roads {
        let (wins, losses, _samples) =
            blue_road_counts(&blue_tree, b_idx, stackelberg.alpha0, stackelberg.beta0);
        let alpha = (stackelberg.alpha0 + wins).max(MIN_BETA_SHAPE);
        let beta = (stackelberg.beta0 + losses).max(MIN_BETA_SHAPE);
        let score = beta_mean(alpha, beta);
        if score > best_blue_leaf_score
            || (score - best_blue_leaf_score).abs() < f64::EPSILON && b_idx < best_blue_idx
        {
            best_blue_leaf_score = score;
            best_blue_idx = b_idx;
        }
    }

    let red_params = PairParams::new(
        &blue_tree,
        &pair_groups,
        stackelberg.alpha0,
        stackelberg.beta0,
        OutcomeFlavor::Red,
    );
    let global_red_means = red_global_settlement_means_with_pool(
        &blue_tree,
        stackelberg.alpha0,
        stackelberg.beta0,
        pooled_red_counts,
    );

    let red_tree = &blue_tree.branches[best_blue_idx].red_tree;
    let mut best_red_settlement_idx = 0usize;
    let mut best_red_score = -1.0;
    for (s_idx, group) in red_tree.settlements.iter().enumerate() {
        let r_idx = group.roads[0];
        let group_idx = pair_groups.index[best_blue_idx][r_idx];
        let red_settlement = group_red_settlements[group_idx];
        let (wins, losses, samples) = settlement_counts(
            red_tree,
            s_idx,
            stackelberg.alpha0,
            stackelberg.beta0,
            OutcomeFlavor::Red,
        );
        let local_p = red_params.group_means[group_idx];
        let global_p = global_red_means
            .get(&red_settlement)
            .copied()
            .unwrap_or(local_p);
        let p_s = if stackelberg.red_global_prior_weight <= 0.0 {
            clamp_prob(local_p)
        } else {
            let weight = stackelberg.red_global_prior_weight
                * (RED_GLOBAL_PRIOR_TAU / (RED_GLOBAL_PRIOR_TAU + samples as f64));
            clamp_prob((1.0 - weight) * local_p + weight * global_p)
        };
        let alpha = (KAPPA * p_s + wins).max(MIN_BETA_SHAPE);
        let beta = (KAPPA * (1.0 - p_s) + losses).max(MIN_BETA_SHAPE);
        let score = beta_mean(alpha, beta);
        if score > best_red_score
            || (score - best_red_score).abs() < f64::EPSILON && s_idx < best_red_settlement_idx
        {
            best_red_score = score;
            best_red_settlement_idx = s_idx;
        }
    }

    let red_roads = &red_tree.settlements[best_red_settlement_idx].roads;
    let mut best_red_idx = red_roads[0];
    let mut best_red_leaf_score = -1.0;
    for &r_idx in red_roads {
        let stats = &red_tree.branches[r_idx].stats;
        let (wins, losses) = road_counts(
            stats,
            stackelberg.alpha0,
            stackelberg.beta0,
            OutcomeFlavor::Red,
        );
        let alpha = (stackelberg.alpha0 + wins).max(MIN_BETA_SHAPE);
        let beta = (stackelberg.beta0 + losses).max(MIN_BETA_SHAPE);
        let score = beta_mean(alpha, beta);
        if score > best_red_leaf_score
            || (score - best_red_leaf_score).abs() < f64::EPSILON && r_idx < best_red_idx
        {
            best_red_leaf_score = score;
            best_red_idx = r_idx;
        }
    }

    let chosen_blue = &blue_tree.branches[best_blue_idx];
    let chosen_red = &chosen_blue.red_tree.branches[best_red_idx];
    let (summary, holdout_sims_run) = if holdout_rerun {
        (
            summarize_playouts(
                board,
                chosen_red.state.as_ref(),
                chosen_red.road_state,
                chosen_red.army_state,
                seeds,
                workers,
                colors,
                max_turns,
            ),
            seeds.len() as u64,
        )
    } else {
        (
            summary_from_leaf(&chosen_red.stats, colors, workers),
            chosen_red.stats.samples,
        )
    };

    if let Some(entries) = all_sims_entries.as_mut() {
        entries.push(AllSimsEntry {
            leader_branch_index,
            leader: leader.clone(),
            leader_second: None,
            followers: vec![chosen_blue.action.clone(), chosen_red.action.clone()],
            summary: summary.clone(),
            sims_run: holdout_sims_run,
            source: "holdout".to_string(),
        });
    }
    if let Some(out) = red_pool_out {
        *out =
            accumulate_red_stats_by_settlement(&blue_tree, stackelberg.alpha0, stackelberg.beta0);
    }

    summary
}
fn summarize_followers_recursive(
    board: &fastcore::board::Board,
    state: &State,
    road_state: RoadState,
    army_state: ArmyState,
    followers: &[PlayerId],
    base_seed: u64,
    seeds: &[u64],
    workers: usize,
    colors: &[String],
    max_turns: u32,
    leader: &PlacementAction,
    leader_branch_index: usize,
    prefix: &mut Vec<PlacementAction>,
    all_sims_entries: &mut Option<Vec<AllSimsEntry>>,
) -> PlayoutSummary {
    if followers.is_empty() {
        let summary = summarize_playouts(
            board, state, road_state, army_state, seeds, workers, colors, max_turns,
        );
        if let Some(entries) = all_sims_entries.as_mut() {
            entries.push(AllSimsEntry {
                leader_branch_index,
                leader: leader.clone(),
                leader_second: None,
                followers: prefix.clone(),
                summary: summary.clone(),
                sims_run: seeds.len() as u64,
                source: "holdout".to_string(),
            });
        }
        return summary;
    }

    let responder = followers[0];
    let settlement_actions = list_settlement_actions(board, state, responder);
    if settlement_actions.is_empty() {
        return summarize_followers_recursive(
            board,
            state,
            road_state,
            army_state,
            &followers[1..],
            base_seed,
            seeds,
            workers,
            colors,
            max_turns,
            leader,
            leader_branch_index,
            prefix,
            all_sims_entries,
        );
    }

    let mut best_summary = None;
    let mut best_score = f64::MIN;
    let mut best_index = 0usize;
    let mut branch_index = 0usize;
    for settlement_action in settlement_actions {
        let mut settled_state = state.clone();
        let mut settled_road = road_state;
        let mut settled_army = army_state;
        let mut settle_rng = rng_for_stream(base_seed, 0);
        apply_value_action(
            board,
            &mut settled_state,
            &mut settled_road,
            &mut settled_army,
            &settlement_action,
            &mut settle_rng,
        );

        let road_actions = list_road_actions(board, &settled_state, responder);
        for road_action in road_actions {
            branch_index += 1;
            let mut branch_state = settled_state.clone();
            let mut branch_road = settled_road;
            let mut branch_army = settled_army;
            let mut branch_rng = rng_for_stream(base_seed, 0);
            apply_value_action(
                board,
                &mut branch_state,
                &mut branch_road,
                &mut branch_army,
                &road_action,
                &mut branch_rng,
            );

            let action = PlacementAction {
                color: colors
                    .get(responder as usize)
                    .cloned()
                    .unwrap_or_else(|| responder.to_string()),
                settlement: match settlement_action.kind {
                    ValueActionKind::BuildSettlement(node) => node,
                    _ => 0,
                },
                road: match road_action.kind {
                    ValueActionKind::BuildRoad(edge) => branch_edge(board, edge),
                    _ => (0, 0),
                },
            };
            prefix.push(action);
            let summary = summarize_followers_recursive(
                board,
                &branch_state,
                branch_road,
                branch_army,
                &followers[1..],
                base_seed,
                seeds,
                workers,
                colors,
                max_turns,
                leader,
                leader_branch_index,
                prefix,
                all_sims_entries,
            );
            prefix.pop();

            let score = summary.win_probabilities[responder as usize];
            if score > best_score
                || (score - best_score).abs() < f64::EPSILON && branch_index < best_index
            {
                best_score = score;
                best_index = branch_index;
                best_summary = Some(summary);
            }
        }
    }

    best_summary.unwrap_or_else(|| {
        summarize_playouts(
            board, state, road_state, army_state, seeds, workers, colors, max_turns,
        )
    })
}

fn effective_workers(requested: usize, seed_count: usize) -> usize {
    #[cfg(feature = "parallel")]
    {
        requested.min(seed_count).max(1)
    }
    #[cfg(not(feature = "parallel"))]
    {
        let _ = (requested, seed_count);
        1
    }
}

fn run_playouts(
    board: &fastcore::board::Board,
    state: &State,
    road_state: RoadState,
    army_state: ArmyState,
    seeds: &[u64],
    workers: usize,
    max_turns: u32,
) -> PlayoutAggregate {
    if workers <= 1 || seeds.len() <= 1 {
        let mut scratch_state = state.clone();
        let players = std::array::from_fn(|_| FastValueFunctionPlayer::new(None, None));
        let mut aggregate = PlayoutAggregate::default();
        for seed in seeds {
            let result = simulate_from_state_with_scratch(
                board,
                state,
                road_state,
                army_state,
                *seed,
                max_turns,
                &mut scratch_state,
                &players,
            );
            aggregate.update(&result);
        }
        return aggregate;
    }

    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        let chunk_size = ((seeds.len() + workers - 1) / workers).max(1);
        return seeds
            .par_chunks(chunk_size)
            .map(|chunk| {
                let mut scratch_state = state.clone();
                let players = std::array::from_fn(|_| FastValueFunctionPlayer::new(None, None));
                let mut aggregate = PlayoutAggregate::default();
                for seed in chunk {
                    let result = simulate_from_state_with_scratch(
                        board,
                        state,
                        road_state,
                        army_state,
                        *seed,
                        max_turns,
                        &mut scratch_state,
                        &players,
                    );
                    aggregate.update(&result);
                }
                aggregate
            })
            .reduce(PlayoutAggregate::default, |mut acc, part| {
                acc.merge(&part);
                acc
            });
    }

    #[cfg(not(feature = "parallel"))]
    {
        let mut scratch_state = state.clone();
        let players = std::array::from_fn(|_| FastValueFunctionPlayer::new(None, None));
        let mut aggregate = PlayoutAggregate::default();
        for seed in seeds {
            let result = simulate_from_state_with_scratch(
                board,
                state,
                road_state,
                army_state,
                *seed,
                max_turns,
                &mut scratch_state,
                &players,
            );
            aggregate.update(&result);
        }
        aggregate
    }
}

fn list_settlement_actions(
    board: &fastcore::board::Board,
    state: &State,
    player: PlayerId,
) -> Vec<ValueAction> {
    generate_playable_actions(board, state, player)
        .into_iter()
        .filter(|action| matches!(action.kind, ValueActionKind::BuildSettlement(_)))
        .collect()
}

fn list_road_actions(
    board: &fastcore::board::Board,
    state: &State,
    player: PlayerId,
) -> Vec<ValueAction> {
    let mut actions: Vec<ValueAction> = generate_playable_actions(board, state, player)
        .into_iter()
        .filter(|action| matches!(action.kind, ValueActionKind::BuildRoad(_)))
        .collect();
    actions.sort_by(|a, b| {
        let edge_a = match a.kind {
            ValueActionKind::BuildRoad(edge) => edge,
            _ => 0,
        };
        let edge_b = match b.kind {
            ValueActionKind::BuildRoad(edge) => edge,
            _ => 0,
        };
        let nodes_a = board.edge_nodes[edge_a as usize];
        let nodes_b = board.edge_nodes[edge_b as usize];
        let pair_a = if nodes_a[0] <= nodes_a[1] {
            (nodes_a[0], nodes_a[1])
        } else {
            (nodes_a[1], nodes_a[0])
        };
        let pair_b = if nodes_b[0] <= nodes_b[1] {
            (nodes_b[0], nodes_b[1])
        } else {
            (nodes_b[1], nodes_b[0])
        };
        pair_a.cmp(&pair_b)
    });
    actions
}

fn branch_edge(board: &fastcore::board::Board, edge: EdgeId) -> (NodeId, NodeId) {
    let nodes = board.edge_nodes[edge as usize];
    if nodes[0] <= nodes[1] {
        (nodes[0], nodes[1])
    } else {
        (nodes[1], nodes[0])
    }
}

fn setup_action_for_color(
    board: &fastcore::board::Board,
    state: &State,
    colors: &[String],
    color: &str,
) -> Option<PlacementAction> {
    let player = color_to_player(colors, color);
    let settlement = state
        .node_owner
        .iter()
        .enumerate()
        .find_map(|(node, owner)| {
            if *owner == player && state.node_level[node] == BuildingLevel::Settlement {
                Some(node as NodeId)
            } else {
                None
            }
        })?;
    let road_edge = state
        .edge_owner
        .iter()
        .enumerate()
        .find_map(|(edge, owner)| {
            if *owner == player {
                Some(edge as EdgeId)
            } else {
                None
            }
        })?;

    Some(PlacementAction {
        color: color.to_string(),
        settlement,
        road: branch_edge(board, road_edge),
    })
}

fn white12_setup_followers(
    board: &fastcore::board::Board,
    state: &State,
    colors: &[String],
) -> Vec<PlacementAction> {
    let mut followers = Vec::new();
    for color in ["ORANGE", "BLUE", "RED"] {
        if let Some(action) = setup_action_for_color(board, state, colors, color) {
            followers.push(action);
        }
    }
    followers
}

fn white12_followers_all_colors(setup_followers: &[PlacementAction]) -> Vec<PlacementAction> {
    setup_followers.to_vec()
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Debug)]
struct SimTask {
    blue_idx: usize,
    red_idx: usize,
    state: Arc<State>,
    road_state: RoadState,
    army_state: ArmyState,
    seed: u64,
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Debug)]
struct SimOutcome {
    blue_idx: usize,
    red_idx: usize,
    result: PlayoutResult,
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Debug)]
struct RoadOption {
    edge_id: EdgeId,
    edge_nodes: (NodeId, NodeId),
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Debug)]
struct BetaStats {
    alpha: f64,
    beta: f64,
    samples: u64,
}

#[cfg(feature = "stackelberg_pruning")]
impl BetaStats {
    fn new(alpha0: f64, beta0: f64) -> Self {
        Self {
            alpha: alpha0,
            beta: beta0,
            samples: 0,
        }
    }

    fn sample<R: Rng>(&self, rng: &mut R) -> f64 {
        sample_beta(rng, self.alpha, self.beta)
    }

    fn mean(&self) -> f64 {
        beta_mean(self.alpha, self.beta)
    }

    fn update(&mut self, result: &PlayoutResult, target: PlayerId) {
        self.samples += 1;
        if result.winner == Some(target) {
            self.alpha += 1.0;
        } else {
            self.beta += 1.0;
        }
    }
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone, Debug)]
struct OrderState {
    first_settlement: NodeId,
    second_settlement: NodeId,
    first_roads: Vec<RoadOption>,
    second_roads: Vec<RoadOption>,
    first_stats: Vec<BetaStats>,
    second_stats: Vec<BetaStats>,
    order_stats: BetaStats,
}

#[cfg(feature = "stackelberg_pruning")]
fn run_sim_tasks(
    board: &fastcore::board::Board,
    tasks: &[SimTask],
    max_turns: u32,
) -> Vec<SimOutcome> {
    if tasks.len() <= 1 {
        let mut scratch_state = tasks
            .first()
            .map(|task| task.state.as_ref().clone())
            .unwrap_or_else(|| State::new());
        let players = std::array::from_fn(|_| FastValueFunctionPlayer::new(None, None));
        return tasks
            .iter()
            .map(|task| SimOutcome {
                blue_idx: task.blue_idx,
                red_idx: task.red_idx,
                result: simulate_from_state_with_scratch(
                    board,
                    task.state.as_ref(),
                    task.road_state,
                    task.army_state,
                    task.seed,
                    max_turns,
                    &mut scratch_state,
                    &players,
                ),
            })
            .collect();
    }

    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        return tasks
            .par_iter()
            .map_init(
                || {
                    let scratch_state = tasks
                        .first()
                        .map(|task| task.state.as_ref().clone())
                        .unwrap_or_else(|| State::new());
                    let players = std::array::from_fn(|_| FastValueFunctionPlayer::new(None, None));
                    (scratch_state, players)
                },
                |(scratch_state, players), task| SimOutcome {
                    blue_idx: task.blue_idx,
                    red_idx: task.red_idx,
                    result: simulate_from_state_with_scratch(
                        board,
                        task.state.as_ref(),
                        task.road_state,
                        task.army_state,
                        task.seed,
                        max_turns,
                        scratch_state,
                        players,
                    ),
                },
            )
            .collect();
    }

    #[cfg(not(feature = "parallel"))]
    {
        let mut scratch_state = tasks
            .first()
            .map(|task| task.state.as_ref().clone())
            .unwrap_or_else(|| State::new());
        let players = std::array::from_fn(|_| FastValueFunctionPlayer::new(None, None));
        tasks
            .iter()
            .map(|task| SimOutcome {
                blue_idx: task.blue_idx,
                red_idx: task.red_idx,
                result: simulate_from_state_with_scratch(
                    board,
                    task.state.as_ref(),
                    task.road_state,
                    task.army_state,
                    task.seed,
                    max_turns,
                    &mut scratch_state,
                    &players,
                ),
            })
            .collect()
    }
}

#[cfg(feature = "stackelberg_pruning")]
fn settlement_node_from_action(action: &ValueAction) -> NodeId {
    match action.kind {
        ValueActionKind::BuildSettlement(node) => node,
        _ => 0,
    }
}

#[cfg(feature = "stackelberg_pruning")]
fn build_red_tree(
    board: &fastcore::board::Board,
    state: &State,
    road_state: RoadState,
    army_state: ArmyState,
    responder: PlayerId,
    base_seed: u64,
    colors: &[String],
    alpha0: f64,
    beta0: f64,
) -> RedTree {
    let mut settlement_actions = list_settlement_actions(board, state, responder);
    settlement_actions.retain(|action| match action.kind {
        ValueActionKind::BuildSettlement(node) => {
            settlement_total_pips(board, node) >= MIN_FOLLOWER_SETTLEMENT_PIPS
        }
        _ => false,
    });
    settlement_actions.sort_by_key(settlement_node_from_action);

    let mut branches = Vec::new();
    let mut settlements = Vec::new();

    for settlement_action in settlement_actions {
        let settlement_node = settlement_node_from_action(&settlement_action);
        let mut settled_state = state.clone();
        let mut settled_road = road_state;
        let mut settled_army = army_state;
        let mut settle_rng = rng_for_stream(base_seed, 0);
        apply_value_action(
            board,
            &mut settled_state,
            &mut settled_road,
            &mut settled_army,
            &settlement_action,
            &mut settle_rng,
        );

        let road_actions = list_road_actions(board, &settled_state, responder);
        let mut road_indices = Vec::new();
        for road_action in road_actions {
            let mut branch_state = settled_state.clone();
            let mut branch_road = settled_road;
            let mut branch_army = settled_army;
            let mut branch_rng = rng_for_stream(base_seed, 0);
            apply_value_action(
                board,
                &mut branch_state,
                &mut branch_road,
                &mut branch_army,
                &road_action,
                &mut branch_rng,
            );

            let road_edge = match road_action.kind {
                ValueActionKind::BuildRoad(edge) => branch_edge(board, edge),
                _ => (0, 0),
            };
            let action = PlacementAction {
                color: colors
                    .get(responder as usize)
                    .cloned()
                    .unwrap_or_else(|| responder.to_string()),
                settlement: settlement_node,
                road: road_edge,
            };
            let idx = branches.len();
            branches.push(RedBranch {
                action,
                state: Arc::new(branch_state),
                road_state: branch_road,
                army_state: branch_army,
                stats: LeafStats::new(alpha0, beta0),
            });
            road_indices.push(idx);
        }
        if !road_indices.is_empty() {
            settlements.push(RedSettlementGroup {
                roads: road_indices,
            });
        }
    }

    if branches.is_empty() {
        let action = PlacementAction {
            color: colors
                .get(responder as usize)
                .cloned()
                .unwrap_or_else(|| responder.to_string()),
            settlement: 0,
            road: (0, 0),
        };
        branches.push(RedBranch {
            action,
            state: Arc::new(state.clone()),
            road_state,
            army_state,
            stats: LeafStats::new(alpha0, beta0),
        });
        settlements.push(RedSettlementGroup { roads: vec![0] });
    }

    RedTree {
        branches,
        settlements,
    }
}

#[cfg(feature = "stackelberg_pruning")]
fn build_blue_tree(
    board: &fastcore::board::Board,
    state: &State,
    road_state: RoadState,
    army_state: ArmyState,
    blue: PlayerId,
    red: PlayerId,
    base_seed: u64,
    colors: &[String],
    alpha0: f64,
    beta0: f64,
) -> BlueTree {
    let mut settlement_actions = list_settlement_actions(board, state, blue);
    settlement_actions.retain(|action| match action.kind {
        ValueActionKind::BuildSettlement(node) => {
            settlement_total_pips(board, node) >= MIN_FOLLOWER_SETTLEMENT_PIPS
        }
        _ => false,
    });
    settlement_actions.sort_by_key(settlement_node_from_action);

    let mut branches = Vec::new();
    let mut settlements = Vec::new();

    for settlement_action in settlement_actions {
        let settlement_node = settlement_node_from_action(&settlement_action);
        let mut settled_state = state.clone();
        let mut settled_road = road_state;
        let mut settled_army = army_state;
        let mut settle_rng = rng_for_stream(base_seed, 0);
        apply_value_action(
            board,
            &mut settled_state,
            &mut settled_road,
            &mut settled_army,
            &settlement_action,
            &mut settle_rng,
        );

        let road_actions = list_road_actions(board, &settled_state, blue);
        let mut road_indices = Vec::new();
        for road_action in road_actions {
            let mut branch_state = settled_state.clone();
            let mut branch_road = settled_road;
            let mut branch_army = settled_army;
            let mut branch_rng = rng_for_stream(base_seed, 0);
            apply_value_action(
                board,
                &mut branch_state,
                &mut branch_road,
                &mut branch_army,
                &road_action,
                &mut branch_rng,
            );

            let road_edge = match road_action.kind {
                ValueActionKind::BuildRoad(edge) => branch_edge(board, edge),
                _ => (0, 0),
            };
            let action = PlacementAction {
                color: colors
                    .get(blue as usize)
                    .cloned()
                    .unwrap_or_else(|| blue.to_string()),
                settlement: settlement_node,
                road: road_edge,
            };
            let red_tree = build_red_tree(
                board,
                &branch_state,
                branch_road,
                branch_army,
                red,
                base_seed,
                colors,
                alpha0,
                beta0,
            );
            let idx = branches.len();
            branches.push(BlueBranch { action, red_tree });
            road_indices.push(idx);
        }
        if !road_indices.is_empty() {
            settlements.push(BlueSettlementGroup {
                roads: road_indices,
            });
        }
    }

    BlueTree {
        branches,
        settlements,
    }
}

#[cfg(feature = "stackelberg_pruning")]
// Top-two Thompson Sampling (TTTS): sample arm posteriors, then choose between the top two.
fn select_red_arm_ttts<R: Rng>(
    tree: &RedTree,
    rho: f64,
    rng: &mut R,
    alpha0: f64,
    beta0: f64,
) -> usize {
    if tree.branches.len() == 1 {
        return 0;
    }
    let settlement_means = settlement_means(tree, alpha0, beta0, OutcomeFlavor::Red);
    let mut settlement_scores = Vec::with_capacity(tree.settlements.len());
    for (s_idx, _group) in tree.settlements.iter().enumerate() {
        let (wins, losses, _samples) =
            settlement_counts(tree, s_idx, alpha0, beta0, OutcomeFlavor::Red);
        let p_s = settlement_means[s_idx];
        let alpha = (KAPPA * p_s + wins).max(MIN_BETA_SHAPE);
        let beta = (KAPPA * (1.0 - p_s) + losses).max(MIN_BETA_SHAPE);
        settlement_scores.push(sample_beta(rng, alpha, beta));
    }

    let (s1, s2) = top_two_indices(&settlement_scores);
    let settlement_choice = choose_top_two(s1, s2, rho, rng);
    let roads = &tree.settlements[settlement_choice].roads;
    let mut best_idx = roads[0];
    let mut best_val = -1.0;
    for &idx in roads {
        let stats = &tree.branches[idx].stats;
        let (wins, losses) = road_counts(stats, alpha0, beta0, OutcomeFlavor::Red);
        let val = sample_plain_beta_from_counts(rng, wins, losses, alpha0, beta0);
        if val > best_val || (val - best_val).abs() < f64::EPSILON && idx < best_idx {
            best_val = val;
            best_idx = idx;
        }
    }
    best_idx
}

#[cfg(feature = "stackelberg_pruning")]
fn select_red_arm_ttts_pair<R: Rng>(
    tree: &BlueTree,
    groups: &PairGroups,
    group_red_settlements: &[NodeId],
    global_red_means: &HashMap<NodeId, f64>,
    global_prior_weight: f64,
    blue_idx: usize,
    rho: f64,
    rng: &mut R,
    red_params: &PairParams,
) -> usize {
    let red_tree = &tree.branches[blue_idx].red_tree;
    if red_tree.branches.len() == 1 {
        return 0;
    }
    let mut settlement_scores = Vec::with_capacity(red_tree.settlements.len());
    for (s_idx, group) in red_tree.settlements.iter().enumerate() {
        let r_idx = group.roads[0];
        let group_idx = groups.index[blue_idx][r_idx];
        let red_settlement = group_red_settlements[group_idx];
        let (wins, losses, samples) = settlement_counts(
            red_tree,
            s_idx,
            red_params.alpha0,
            red_params.beta0,
            OutcomeFlavor::Red,
        );
        let local_p = red_params.group_means[group_idx];
        let global_p = global_red_means
            .get(&red_settlement)
            .copied()
            .unwrap_or(local_p);
        let p_s = if global_prior_weight <= 0.0 {
            clamp_prob(local_p)
        } else {
            let weight = global_prior_weight
                * (RED_GLOBAL_PRIOR_TAU / (RED_GLOBAL_PRIOR_TAU + samples as f64));
            clamp_prob((1.0 - weight) * local_p + weight * global_p)
        };
        let alpha = (red_params.kappa * p_s + wins).max(MIN_BETA_SHAPE);
        let beta = (red_params.kappa * (1.0 - p_s) + losses).max(MIN_BETA_SHAPE);
        settlement_scores.push(sample_beta(rng, alpha, beta));
    }

    let (s1, s2) = top_two_indices(&settlement_scores);
    let settlement_choice = choose_top_two(s1, s2, rho, rng);
    let roads = &red_tree.settlements[settlement_choice].roads;
    let mut best_idx = roads[0];
    let mut best_val = -1.0;
    for &idx in roads {
        let stats = &red_tree.branches[idx].stats;
        let (wins, losses) = road_counts(
            stats,
            red_params.alpha0,
            red_params.beta0,
            OutcomeFlavor::Red,
        );
        let val =
            sample_plain_beta_from_counts(rng, wins, losses, red_params.alpha0, red_params.beta0);
        if val > best_val || (val - best_val).abs() < f64::EPSILON && idx < best_idx {
            best_val = val;
            best_idx = idx;
        }
    }
    best_idx
}

#[cfg(feature = "stackelberg_pruning")]
fn select_blue_red_arm_ttts<R: Rng>(
    tree: &BlueTree,
    groups: &PairGroups,
    group_red_settlements: &[NodeId],
    global_red_means: &HashMap<NodeId, f64>,
    global_prior_weight: f64,
    rho: f64,
    rng: &mut R,
    alpha0: f64,
    beta0: f64,
) -> (usize, usize) {
    if tree.branches.len() == 1 {
        let red_params = PairParams::new(tree, groups, alpha0, beta0, OutcomeFlavor::Red);
        let red_idx = select_red_arm_ttts_pair(
            tree,
            groups,
            group_red_settlements,
            global_red_means,
            global_prior_weight,
            0,
            rho,
            rng,
            &red_params,
        );
        return (0, red_idx);
    }

    let mut settlement_scores = Vec::with_capacity(tree.settlements.len());
    for s_idx in 0..tree.settlements.len() {
        let (wins, losses, _samples) = blue_settlement_counts(tree, s_idx, alpha0, beta0);
        settlement_scores.push(sample_plain_beta_from_counts(
            rng, wins, losses, alpha0, beta0,
        ));
    }

    let (s1, s2) = top_two_indices(&settlement_scores);
    let settlement_choice = choose_top_two(s1, s2, rho, rng);
    let roads = &tree.settlements[settlement_choice].roads;
    let mut best_blue_idx = roads[0];
    let mut best_blue_val = -1.0;
    for &b_idx in roads {
        let (wins, losses, _samples) = blue_road_counts(tree, b_idx, alpha0, beta0);
        let val = sample_plain_beta_from_counts(rng, wins, losses, alpha0, beta0);
        if val > best_blue_val
            || (val - best_blue_val).abs() < f64::EPSILON && b_idx < best_blue_idx
        {
            best_blue_val = val;
            best_blue_idx = b_idx;
        }
    }

    let red_params = PairParams::new(tree, groups, alpha0, beta0, OutcomeFlavor::Red);
    let red_choice = select_red_arm_ttts_pair(
        tree,
        groups,
        group_red_settlements,
        global_red_means,
        global_prior_weight,
        best_blue_idx,
        rho,
        rng,
        &red_params,
    );
    (best_blue_idx, red_choice)
}

#[cfg(feature = "stackelberg_pruning")]
fn build_flat_indices(tree: &BlueTree) -> (Vec<(usize, usize)>, Vec<Vec<usize>>) {
    let mut flat = Vec::new();
    let mut index = Vec::with_capacity(tree.branches.len());
    for (b_idx, branch) in tree.branches.iter().enumerate() {
        let mut row = Vec::with_capacity(branch.red_tree.branches.len());
        for r_idx in 0..branch.red_tree.branches.len() {
            row.push(flat.len());
            flat.push((b_idx, r_idx));
        }
        index.push(row);
    }
    (flat, index)
}

#[cfg(feature = "stackelberg_pruning")]
fn next_warm_red_settlement<R: Rng>(
    tree: &RedTree,
    pending_settlements: &mut [u64],
    min_samples: u64,
    warm: &mut WarmStartState,
    rng: &mut R,
    alpha0: f64,
    beta0: f64,
) -> Option<usize> {
    if min_samples == 0 || tree.settlements.is_empty() {
        return None;
    }
    let total = tree.settlements.len();
    for offset in 0..total {
        let s_idx = (warm.next_idx + offset) % total;
        let samples = red_settlement_samples(tree, s_idx);
        if samples + pending_settlements[s_idx] < min_samples {
            pending_settlements[s_idx] += 1;
            warm.next_idx = (s_idx + 1) % total;
            let roads = &tree.settlements[s_idx].roads;
            let mut best_idx = roads[0];
            let mut best_val = -1.0;
            for &idx in roads {
                let stats = &tree.branches[idx].stats;
                let (wins, losses) = road_counts(stats, alpha0, beta0, OutcomeFlavor::Red);
                let val = sample_plain_beta_from_counts(rng, wins, losses, alpha0, beta0);
                if val > best_val || (val - best_val).abs() < f64::EPSILON && idx < best_idx {
                    best_val = val;
                    best_idx = idx;
                }
            }
            return Some(best_idx);
        }
    }
    None
}

#[cfg(feature = "stackelberg_pruning")]
fn next_warm_blue_settlement<R: Rng>(
    tree: &BlueTree,
    groups: &PairGroups,
    group_red_settlements: &[NodeId],
    global_red_means: &HashMap<NodeId, f64>,
    global_prior_weight: f64,
    pending_settlements: &mut [u64],
    min_samples: u64,
    warm: &mut WarmStartState,
    rng: &mut R,
    alpha0: f64,
    beta0: f64,
    rho: f64,
) -> Option<(usize, usize)> {
    if min_samples == 0 || tree.settlements.is_empty() {
        return None;
    }
    let total = tree.settlements.len();
    let red_params = PairParams::new(tree, groups, alpha0, beta0, OutcomeFlavor::Red);
    for offset in 0..total {
        let s_idx = (warm.next_idx + offset) % total;
        let samples = blue_settlement_samples(tree, s_idx);
        if samples + pending_settlements[s_idx] < min_samples {
            pending_settlements[s_idx] += 1;
            warm.next_idx = (s_idx + 1) % total;
            let roads = &tree.settlements[s_idx].roads;
            let mut best_blue = roads[0];
            let mut best_val = -1.0;
            for &b_idx in roads {
                let (wins, losses, _samples) = blue_road_counts(tree, b_idx, alpha0, beta0);
                let val = sample_plain_beta_from_counts(rng, wins, losses, alpha0, beta0);
                if val > best_val || (val - best_val).abs() < f64::EPSILON && b_idx < best_blue {
                    best_val = val;
                    best_blue = b_idx;
                }
            }
            let red_idx = select_red_arm_ttts_pair(
                tree,
                groups,
                group_red_settlements,
                global_red_means,
                global_prior_weight,
                best_blue,
                rho,
                rng,
                &red_params,
            );
            return Some((best_blue, red_idx));
        }
    }
    None
}

fn build_branch_tasks(
    board: &fastcore::board::Board,
    base_state: &State,
    base_road: RoadState,
    base_army: ArmyState,
    seed: u64,
    limit: Option<usize>,
    leader_settlement: Option<NodeId>,
) -> Vec<BranchTask> {
    let mut settlement_actions =
        list_settlement_actions(board, base_state, base_state.active_player);
    if let Some(target_node) = leader_settlement {
        settlement_actions.retain(|action| match action.kind {
            ValueActionKind::BuildSettlement(node) => node == target_node,
            _ => false,
        });
    }
    if settlement_actions.is_empty() {
        panic!("no settlement actions available in the provided state");
    }

    let mut tasks = Vec::new();
    let mut branch_index = 0usize;
    for settlement_action in settlement_actions {
        let mut settled_state = base_state.clone();
        let mut settled_road = base_road;
        let mut settled_army = base_army;
        let mut settle_rng = rng_for_stream(seed, 0);
        apply_value_action(
            board,
            &mut settled_state,
            &mut settled_road,
            &mut settled_army,
            &settlement_action,
            &mut settle_rng,
        );

        let pips = color_resource_pips(board, &settled_state, settlement_action.player);
        let road_actions = list_road_actions(board, &settled_state, settlement_action.player);
        for road_action in road_actions {
            branch_index += 1;
            let edge_id = match road_action.kind {
                ValueActionKind::BuildRoad(edge) => edge,
                _ => continue,
            };
            let edge_nodes = branch_edge(board, edge_id);
            let settlement_node = match settlement_action.kind {
                ValueActionKind::BuildSettlement(node) => node,
                _ => 0,
            };
            tasks.push(BranchTask {
                branch_index,
                player: settlement_action.player,
                settlement_node,
                road_edge_id: edge_id,
                road_edge_nodes: edge_nodes,
                pips,
            });

            if let Some(limit) = limit {
                if branch_index >= limit {
                    break;
                }
            }
        }
        if let Some(limit) = limit {
            if branch_index >= limit {
                break;
            }
        }
    }
    tasks
}

fn build_white12_tasks(
    board: &fastcore::board::Board,
    base_state: &State,
    base_road: RoadState,
    base_army: ArmyState,
    seed: u64,
    limit: Option<usize>,
    leader_settlement: Option<NodeId>,
    player: PlayerId,
) -> Vec<WhitePairTask> {
    let mut state_for_actions = base_state.clone();
    prime_initial_settlement_state(&mut state_for_actions, player);
    let mut settlement_actions = list_settlement_actions(board, &state_for_actions, player);
    settlement_actions.retain(|action| match action.kind {
        ValueActionKind::BuildSettlement(_) => true,
        _ => false,
    });
    settlement_actions.sort_by_key(|action| match action.kind {
        ValueActionKind::BuildSettlement(node) => node,
        _ => 0,
    });
    if settlement_actions.is_empty() {
        panic!("no settlement actions available for WHITE12 in the provided state");
    }

    let mut settlements = Vec::new();
    for action in settlement_actions {
        let node = match action.kind {
            ValueActionKind::BuildSettlement(node) => node,
            _ => continue,
        };
        let pips = settlement_pips(board, node);
        settlements.push((node, pips));
    }
    if settlements.len() < 2 {
        panic!("not enough settlement candidates to build WHITE12 pairs");
    }

    let mut tasks = Vec::new();
    let mut branch_index = 0usize;
    for i in 0..settlements.len() {
        for j in (i + 1)..settlements.len() {
            let (a_node, a_pips) = settlements[i];
            let (b_node, b_pips) = settlements[j];
            if let Some(target) = leader_settlement {
                if a_node != target && b_node != target {
                    continue;
                }
            }
            if !is_settlement_pair_legal(
                board, base_state, base_road, base_army, seed, player, a_node, b_node,
            ) {
                continue;
            }
            branch_index += 1;
            tasks.push(WhitePairTask {
                branch_index,
                player,
                settlement_a: a_node,
                settlement_b: b_node,
                pips_a: a_pips,
                pips_b: b_pips,
            });
            if let Some(limit) = limit {
                if branch_index >= limit {
                    return tasks;
                }
            }
        }
    }
    tasks
}

fn is_settlement_pair_legal(
    board: &fastcore::board::Board,
    base_state: &State,
    base_road: RoadState,
    base_army: ArmyState,
    seed: u64,
    player: PlayerId,
    first: NodeId,
    second: NodeId,
) -> bool {
    let mut state = base_state.clone();
    let mut road_state = base_road;
    let mut army_state = base_army;
    prime_initial_settlement_state(&mut state, player);
    let mut rng = rng_for_stream(seed, 0);
    let action = ValueAction {
        player,
        kind: ValueActionKind::BuildSettlement(first),
    };
    apply_value_action(
        board,
        &mut state,
        &mut road_state,
        &mut army_state,
        &action,
        &mut rng,
    );
    prime_initial_settlement_state(&mut state, player);
    let actions = list_settlement_actions(board, &state, player);
    actions.iter().any(|action| match action.kind {
        ValueActionKind::BuildSettlement(node) => node == second,
        _ => false,
    })
}

fn prime_initial_settlement_state(state: &mut State, player: PlayerId) {
    state.active_player = player;
    state.turn_player = player;
    state.is_initial_build_phase = true;
    state.current_prompt = ActionPrompt::BuildInitialSettlement;
}

#[cfg(feature = "stackelberg_pruning")]
fn settlement_road_options(
    board: &fastcore::board::Board,
    base_state: &State,
    base_road: RoadState,
    base_army: ArmyState,
    seed: u64,
    player: PlayerId,
    settlement: NodeId,
) -> Vec<RoadOption> {
    let mut state = base_state.clone();
    let mut road_state = base_road;
    let mut army_state = base_army;
    prime_initial_settlement_state(&mut state, player);
    let mut rng = rng_for_stream(seed, 0);
    let action = ValueAction {
        player,
        kind: ValueActionKind::BuildSettlement(settlement),
    };
    apply_value_action(
        board,
        &mut state,
        &mut road_state,
        &mut army_state,
        &action,
        &mut rng,
    );
    let road_actions = list_road_actions(board, &state, player);
    road_actions
        .into_iter()
        .filter_map(|action| match action.kind {
            ValueActionKind::BuildRoad(edge) => Some(RoadOption {
                edge_id: edge,
                edge_nodes: branch_edge(board, edge),
            }),
            _ => None,
        })
        .collect()
}

#[cfg(feature = "stackelberg_pruning")]
fn select_ts_index<R: Rng>(stats: &[BetaStats], rng: &mut R) -> usize {
    let mut best_idx = 0usize;
    let mut best_val = -1.0;
    for (idx, stat) in stats.iter().enumerate() {
        let val = stat.sample(rng);
        if val > best_val || (val - best_val).abs() < f64::EPSILON && idx < best_idx {
            best_val = val;
            best_idx = idx;
        }
    }
    best_idx
}

#[cfg(feature = "stackelberg_pruning")]
fn select_order_idx<R: Rng>(orders: &[OrderState], min_samples: u64, rng: &mut R) -> usize {
    if min_samples > 0 {
        for (idx, order) in orders.iter().enumerate() {
            if order.order_stats.samples < min_samples {
                return idx;
            }
        }
    }
    let mut best_idx = 0usize;
    let mut best_val = -1.0;
    for (idx, order) in orders.iter().enumerate() {
        let val = order.order_stats.sample(rng);
        if val > best_val || (val - best_val).abs() < f64::EPSILON && idx < best_idx {
            best_val = val;
            best_idx = idx;
        }
    }
    best_idx
}

#[cfg(feature = "stackelberg_pruning")]
fn best_mean_index(stats: &[BetaStats]) -> usize {
    let mut best_idx = 0usize;
    let mut best_val = -1.0;
    for (idx, stat) in stats.iter().enumerate() {
        let val = stat.mean();
        if val > best_val || (val - best_val).abs() < f64::EPSILON && idx < best_idx {
            best_val = val;
            best_idx = idx;
        }
    }
    best_idx
}

#[cfg(feature = "stackelberg_pruning")]
fn build_white12_state(
    board: &fastcore::board::Board,
    base_state: &State,
    base_road: RoadState,
    base_army: ArmyState,
    seed: u64,
    player: PlayerId,
    order: &OrderState,
    first_road_idx: usize,
    second_road_idx: usize,
) -> (State, RoadState, ArmyState) {
    let mut state = base_state.clone();
    let mut road_state = base_road;
    let mut army_state = base_army;
    let mut rng = rng_for_stream(seed, 0);

    state.active_player = player;
    state.turn_player = player;
    state.is_initial_build_phase = true;
    state.current_prompt = ActionPrompt::BuildInitialSettlement;
    let first_settle = ValueAction {
        player,
        kind: ValueActionKind::BuildSettlement(order.first_settlement),
    };
    apply_value_action(
        board,
        &mut state,
        &mut road_state,
        &mut army_state,
        &first_settle,
        &mut rng,
    );
    let first_road = ValueAction {
        player,
        kind: ValueActionKind::BuildRoad(order.first_roads[first_road_idx].edge_id),
    };
    apply_value_action(
        board,
        &mut state,
        &mut road_state,
        &mut army_state,
        &first_road,
        &mut rng,
    );

    state.active_player = player;
    state.turn_player = player;
    state.is_initial_build_phase = true;
    state.current_prompt = ActionPrompt::BuildInitialSettlement;
    let second_settle = ValueAction {
        player,
        kind: ValueActionKind::BuildSettlement(order.second_settlement),
    };
    apply_value_action(
        board,
        &mut state,
        &mut road_state,
        &mut army_state,
        &second_settle,
        &mut rng,
    );
    let second_road = ValueAction {
        player,
        kind: ValueActionKind::BuildRoad(order.second_roads[second_road_idx].edge_id),
    };
    apply_value_action(
        board,
        &mut state,
        &mut road_state,
        &mut army_state,
        &second_road,
        &mut rng,
    );

    (state, road_state, army_state)
}

fn evaluate_branch_task(
    board: &fastcore::board::Board,
    base_state: &State,
    base_road: RoadState,
    base_army: ArmyState,
    base_seed: u64,
    task: &BranchTask,
    seeds: &[u64],
    workers: usize,
    colors: &[String],
    sort_color_idx: usize,
    followers: &[PlayerId],
    max_turns: u32,
    stackelberg: &StackelbergConfig,
    holdout_rerun: bool,
    include_ts_entries: bool,
    pooled_red_counts: Option<&RedPooledCounts>,
) -> BranchResult {
    #[cfg(not(feature = "stackelberg_pruning"))]
    touch_stackelberg_config(stackelberg);
    #[cfg(not(feature = "stackelberg_pruning"))]
    let _ = pooled_red_counts;
    #[cfg(not(feature = "stackelberg_pruning"))]
    let _ = include_ts_entries;
    let mut branch_state = base_state.clone();
    let mut branch_road = base_road;
    let mut branch_army = base_army;
    let mut settle_rng = rng_for_stream(base_seed, 0);
    let settlement_action = ValueAction {
        player: task.player,
        kind: ValueActionKind::BuildSettlement(task.settlement_node),
    };
    apply_value_action(
        board,
        &mut branch_state,
        &mut branch_road,
        &mut branch_army,
        &settlement_action,
        &mut settle_rng,
    );
    let mut road_rng = rng_for_stream(base_seed, 0);
    let road_action = ValueAction {
        player: task.player,
        kind: ValueActionKind::BuildRoad(task.road_edge_id),
    };
    apply_value_action(
        board,
        &mut branch_state,
        &mut branch_road,
        &mut branch_army,
        &road_action,
        &mut road_rng,
    );

    let leader_action = PlacementAction {
        color: colors
            .get(task.player as usize)
            .cloned()
            .unwrap_or_else(|| task.player.to_string()),
        settlement: task.settlement_node,
        road: task.road_edge_nodes,
    };

    let mut all_sims_entries = if followers.is_empty() {
        None
    } else {
        Some(Vec::new())
    };
    #[cfg(feature = "stackelberg_pruning")]
    let mut red_pool_stats: RedPooledCounts = HashMap::new();

    let summary = if followers.is_empty() {
        summarize_playouts(
            board,
            &branch_state,
            branch_road,
            branch_army,
            seeds,
            workers,
            colors,
            max_turns,
        )
    } else {
        #[cfg(feature = "stackelberg_pruning")]
        {
            summarize_with_followers_pruned(
                board,
                &branch_state,
                branch_road,
                branch_army,
                followers,
                base_seed,
                seeds,
                workers,
                colors,
                max_turns,
                &leader_action,
                task.branch_index,
                &mut all_sims_entries,
                include_ts_entries,
                stackelberg,
                holdout_rerun,
                pooled_red_counts,
                Some(&mut red_pool_stats),
            )
        }
        #[cfg(not(feature = "stackelberg_pruning"))]
        {
            summarize_with_followers(
                board,
                &branch_state,
                branch_road,
                branch_army,
                followers,
                base_seed,
                seeds,
                workers,
                colors,
                max_turns,
                &leader_action,
                task.branch_index,
                &mut all_sims_entries,
            )
        }
    };

    let score = summary.win_probabilities[sort_color_idx];
    let evaluation = BranchEvaluation {
        score,
        branch_index: task.branch_index,
        settlement_node: task.settlement_node,
        road_edge: task.road_edge_nodes,
        settlement_node2: None,
        road_edge2: None,
        pips: task.pips,
        pips2: None,
        summary,
    };

    BranchResult {
        evaluation,
        all_sims_entries: all_sims_entries.unwrap_or_default(),
        #[cfg(feature = "stackelberg_pruning")]
        red_pool_stats,
    }
}

fn evaluate_branch_task_dry_run(
    board: &fastcore::board::Board,
    base_state: &State,
    base_road: RoadState,
    base_army: ArmyState,
    base_seed: u64,
    task: &BranchTask,
    colors: &[String],
    sort_color_idx: usize,
    followers: &[PlayerId],
    stackelberg: &StackelbergConfig,
) -> BranchResult {
    #[cfg(not(feature = "stackelberg_pruning"))]
    touch_stackelberg_config(stackelberg);
    let mut branch_state = base_state.clone();
    let mut branch_road = base_road;
    let mut branch_army = base_army;
    let mut settle_rng = rng_for_stream(base_seed, 0);
    let settlement_action = ValueAction {
        player: task.player,
        kind: ValueActionKind::BuildSettlement(task.settlement_node),
    };
    apply_value_action(
        board,
        &mut branch_state,
        &mut branch_road,
        &mut branch_army,
        &settlement_action,
        &mut settle_rng,
    );
    let mut road_rng = rng_for_stream(base_seed, 0);
    let road_action = ValueAction {
        player: task.player,
        kind: ValueActionKind::BuildRoad(task.road_edge_id),
    };
    apply_value_action(
        board,
        &mut branch_state,
        &mut branch_road,
        &mut branch_army,
        &road_action,
        &mut road_rng,
    );

    let leader_action = PlacementAction {
        color: colors
            .get(task.player as usize)
            .cloned()
            .unwrap_or_else(|| task.player.to_string()),
        settlement: task.settlement_node,
        road: task.road_edge_nodes,
    };
    let summary = dry_run_summary(colors);
    let mut all_sims_entries = Vec::new();

    #[cfg(feature = "stackelberg_pruning")]
    {
        if followers.len() == 1 {
            let responder = followers[0];
            let red_tree = build_red_tree(
                board,
                &branch_state,
                branch_road,
                branch_army,
                responder,
                base_seed,
                colors,
                stackelberg.alpha0,
                stackelberg.beta0,
            );
            if !red_tree.branches.is_empty() {
                let idx = 0usize;
                all_sims_entries.push(AllSimsEntry {
                    leader_branch_index: task.branch_index,
                    leader: leader_action.clone(),
                    leader_second: None,
                    followers: vec![red_tree.branches[idx].action.clone()],
                    summary: summary.clone(),
                    sims_run: 0,
                    source: "holdout".to_string(),
                });
            }
        } else if followers.len() == 2 {
            let blue = followers[0];
            let red = followers[1];
            let blue_tree = build_blue_tree(
                board,
                &branch_state,
                branch_road,
                branch_army,
                blue,
                red,
                base_seed,
                colors,
                stackelberg.alpha0,
                stackelberg.beta0,
            );
            if !blue_tree.branches.is_empty() {
                let blue_idx = 0usize;
                let red_tree = &blue_tree.branches[blue_idx].red_tree;
                if !red_tree.branches.is_empty() {
                    let red_idx = 0usize;
                    all_sims_entries.push(AllSimsEntry {
                        leader_branch_index: task.branch_index,
                        leader: leader_action.clone(),
                        leader_second: None,
                        followers: vec![
                            blue_tree.branches[blue_idx].action.clone(),
                            red_tree.branches[red_idx].action.clone(),
                        ],
                        summary: summary.clone(),
                        sims_run: 0,
                        source: "holdout".to_string(),
                    });
                }
            }
        }
    }

    let score = summary.win_probabilities[sort_color_idx];
    let evaluation = BranchEvaluation {
        score,
        branch_index: task.branch_index,
        settlement_node: task.settlement_node,
        road_edge: task.road_edge_nodes,
        settlement_node2: None,
        road_edge2: None,
        pips: task.pips,
        pips2: None,
        summary,
    };

    BranchResult {
        evaluation,
        all_sims_entries,
        #[cfg(feature = "stackelberg_pruning")]
        red_pool_stats: HashMap::new(),
    }
}

#[cfg(feature = "stackelberg_pruning")]
#[derive(Clone)]
struct White12PreparedState {
    state: Arc<State>,
    road_state: RoadState,
    army_state: ArmyState,
}

#[cfg(feature = "stackelberg_pruning")]
fn evaluate_white12_task(
    board: &fastcore::board::Board,
    base_state: &State,
    base_road: RoadState,
    base_army: ArmyState,
    base_seed: u64,
    task: &WhitePairTask,
    seeds: &[u64],
    workers: usize,
    colors: &[String],
    sort_color_idx: usize,
    max_turns: u32,
    stackelberg: &StackelbergConfig,
    holdout_rerun: bool,
    include_ts_entries: bool,
) -> BranchResult {
    let player = task.player;
    let target_player = sort_color_idx as PlayerId;
    let leader_color = colors
        .get(player as usize)
        .cloned()
        .unwrap_or_else(|| player.to_string());
    let setup_followers = white12_setup_followers(board, base_state, colors);
    let roads_a = settlement_road_options(
        board,
        base_state,
        base_road,
        base_army,
        base_seed,
        player,
        task.settlement_a,
    );
    let roads_b = settlement_road_options(
        board,
        base_state,
        base_road,
        base_army,
        base_seed,
        player,
        task.settlement_b,
    );

    if roads_a.is_empty() || roads_b.is_empty() {
        let summary = summarize_playouts(
            board, base_state, base_road, base_army, seeds, workers, colors, max_turns,
        );
        return BranchResult {
            evaluation: BranchEvaluation {
                score: summary.win_probabilities[sort_color_idx],
                branch_index: task.branch_index,
                settlement_node: task.settlement_a,
                road_edge: (0, 0),
                settlement_node2: Some(task.settlement_b),
                road_edge2: Some((0, 0)),
                pips: task.pips_a,
                pips2: Some(task.pips_b),
                summary,
            },
            all_sims_entries: Vec::new(),
            red_pool_stats: HashMap::new(),
        };
    }

    let mut combo_stats: Vec<Vec<Vec<LeafStats>>> = Vec::new();
    let mut orders = vec![
        OrderState {
            first_settlement: task.settlement_a,
            second_settlement: task.settlement_b,
            first_roads: roads_a.clone(),
            second_roads: roads_b.clone(),
            first_stats: vec![BetaStats::new(stackelberg.alpha0, stackelberg.beta0); roads_a.len()],
            second_stats: vec![
                BetaStats::new(stackelberg.alpha0, stackelberg.beta0);
                roads_b.len()
            ],
            order_stats: BetaStats::new(stackelberg.alpha0, stackelberg.beta0),
        },
        OrderState {
            first_settlement: task.settlement_b,
            second_settlement: task.settlement_a,
            first_roads: roads_b.clone(),
            second_roads: roads_a.clone(),
            first_stats: vec![BetaStats::new(stackelberg.alpha0, stackelberg.beta0); roads_b.len()],
            second_stats: vec![
                BetaStats::new(stackelberg.alpha0, stackelberg.beta0);
                roads_a.len()
            ],
            order_stats: BetaStats::new(stackelberg.alpha0, stackelberg.beta0),
        },
    ];
    for order in &orders {
        let mut order_stats = Vec::with_capacity(order.first_roads.len());
        for _ in 0..order.first_roads.len() {
            order_stats.push(vec![
                LeafStats::new(stackelberg.alpha0, stackelberg.beta0);
                order.second_roads.len()
            ]);
        }
        combo_stats.push(order_stats);
    }

    let base_seed_l = base_seed ^ (task.branch_index as u64).wrapping_mul(0x9E3779B97F4A7C15);
    let prepared_states: Vec<Vec<Vec<White12PreparedState>>> = orders
        .iter()
        .map(|order| {
            (0..order.first_roads.len())
                .map(|road1_idx| {
                    (0..order.second_roads.len())
                        .map(|road2_idx| {
                            let (state, road_state, army_state) = build_white12_state(
                                board,
                                base_state,
                                base_road,
                                base_army,
                                base_seed_l,
                                player,
                                order,
                                road1_idx,
                                road2_idx,
                            );
                            White12PreparedState {
                                state: Arc::new(state),
                                road_state,
                                army_state,
                            }
                        })
                        .collect()
                })
                .collect()
        })
        .collect();
    let mut rng = rng_for_stream(base_seed_l, 1);
    let mut pair_stats = BetaStats::new(stackelberg.alpha0, stackelberg.beta0);
    let mut all_sims_entries = Vec::new();
    let mut remaining = stackelberg.budget.max(1);
    while remaining > 0 {
        let mut batch = stackelberg.batch_sims.max(1).min(remaining as usize);
        if batch == 1 && remaining > 1 {
            batch = 2;
        }
        let mut pending = 0u64;
        let mut tasks = Vec::with_capacity(batch);
        let mut meta = Vec::with_capacity(batch);
        for task_idx in 0..batch {
            let order_idx = select_order_idx(&orders, stackelberg.min_samples, &mut rng);
            let road1_idx = select_ts_index(&orders[order_idx].first_stats, &mut rng);
            let road2_idx = select_ts_index(&orders[order_idx].second_stats, &mut rng);
            let prepared = &prepared_states[order_idx][road1_idx][road2_idx];
            let t = pair_stats.samples + pending;
            pending += 1;
            let seed = stackelberg_seed(base_seed_l, order_idx, road1_idx, t);
            tasks.push(SimTask {
                blue_idx: task_idx,
                red_idx: 0,
                state: Arc::clone(&prepared.state),
                road_state: prepared.road_state,
                army_state: prepared.army_state,
                seed,
            });
            meta.push((order_idx, road1_idx, road2_idx));
        }
        let outcomes = run_sim_tasks(board, &tasks, max_turns);
        for outcome in outcomes {
            let (order_idx, road1_idx, road2_idx) = meta[outcome.blue_idx];
            let result = outcome.result;
            pair_stats.update(&result, target_player);
            let order = &mut orders[order_idx];
            order.order_stats.update(&result, target_player);
            order.first_stats[road1_idx].update(&result, target_player);
            order.second_stats[road2_idx].update(&result, target_player);
            combo_stats[order_idx][road1_idx][road2_idx].update(&result, target_player, None);
        }
        remaining = remaining.saturating_sub(batch as u64);
    }

    if include_ts_entries {
        for (order_idx, order) in orders.iter().enumerate() {
            for (road1_idx, road2_stats) in combo_stats[order_idx].iter().enumerate() {
                for (_road2_idx, stats) in road2_stats.iter().enumerate() {
                    if stats.samples == 0 {
                        continue;
                    }
                    let summary = summary_from_leaf(stats, colors, workers);
                    let leader_action = PlacementAction {
                        color: leader_color.clone(),
                        settlement: order.first_settlement,
                        road: order.first_roads[road1_idx].edge_nodes,
                    };
                    let leader_second_action = PlacementAction {
                        color: leader_color.clone(),
                        settlement: order.second_settlement,
                        road: order.second_roads[_road2_idx].edge_nodes,
                    };
                    all_sims_entries.push(AllSimsEntry {
                        leader_branch_index: task.branch_index,
                        leader: leader_action,
                        leader_second: Some(leader_second_action),
                        followers: white12_followers_all_colors(&setup_followers),
                        summary,
                        sims_run: stats.samples,
                        source: "ts".to_string(),
                    });
                }
            }
        }
    }

    let mut best_order_idx = 0usize;
    let mut best_order_score = -1.0;
    for (idx, order) in orders.iter().enumerate() {
        let score = order.order_stats.mean();
        if score > best_order_score
            || (score - best_order_score).abs() < f64::EPSILON && idx < best_order_idx
        {
            best_order_score = score;
            best_order_idx = idx;
        }
    }
    let best_order = &orders[best_order_idx];
    let best_road1_idx = best_mean_index(&best_order.first_stats);
    let best_road2_idx = best_mean_index(&best_order.second_stats);

    let chosen_stats = &combo_stats[best_order_idx][best_road1_idx][best_road2_idx];
    let summary = if holdout_rerun {
        let prepared = &prepared_states[best_order_idx][best_road1_idx][best_road2_idx];
        summarize_playouts(
            board,
            prepared.state.as_ref(),
            prepared.road_state,
            prepared.army_state,
            seeds,
            workers,
            colors,
            max_turns,
        )
    } else {
        summary_from_leaf(chosen_stats, colors, workers)
    };

    if holdout_rerun {
        for (order_idx, order) in orders.iter().enumerate() {
            for road1_idx in 0..order.first_roads.len() {
                for road2_idx in 0..order.second_roads.len() {
                    let prepared = &prepared_states[order_idx][road1_idx][road2_idx];
                    let hold_summary = summarize_playouts(
                        board,
                        prepared.state.as_ref(),
                        prepared.road_state,
                        prepared.army_state,
                        seeds,
                        workers,
                        colors,
                        max_turns,
                    );
                    let leader_action = PlacementAction {
                        color: leader_color.clone(),
                        settlement: order.first_settlement,
                        road: order.first_roads[road1_idx].edge_nodes,
                    };
                    let leader_second_action = PlacementAction {
                        color: leader_color.clone(),
                        settlement: order.second_settlement,
                        road: order.second_roads[road2_idx].edge_nodes,
                    };
                    all_sims_entries.push(AllSimsEntry {
                        leader_branch_index: task.branch_index,
                        leader: leader_action,
                        leader_second: Some(leader_second_action),
                        followers: white12_followers_all_colors(&setup_followers),
                        summary: hold_summary,
                        sims_run: seeds.len() as u64,
                        source: "holdout".to_string(),
                    });
                }
            }
        }
    } else {
        let leader_action = PlacementAction {
            color: leader_color.clone(),
            settlement: best_order.first_settlement,
            road: best_order.first_roads[best_road1_idx].edge_nodes,
        };
        let leader_second_action = PlacementAction {
            color: leader_color.clone(),
            settlement: best_order.second_settlement,
            road: best_order.second_roads[best_road2_idx].edge_nodes,
        };
        all_sims_entries.push(AllSimsEntry {
            leader_branch_index: task.branch_index,
            leader: leader_action,
            leader_second: Some(leader_second_action),
            followers: white12_followers_all_colors(&setup_followers),
            summary: summary.clone(),
            sims_run: chosen_stats.samples,
            source: "holdout".to_string(),
        });
    }

    let (pips_first, pips_second) = if best_order.first_settlement == task.settlement_a {
        (task.pips_a, task.pips_b)
    } else {
        (task.pips_b, task.pips_a)
    };

    BranchResult {
        evaluation: BranchEvaluation {
            score: summary.win_probabilities[sort_color_idx],
            branch_index: task.branch_index,
            settlement_node: best_order.first_settlement,
            road_edge: best_order.first_roads[best_road1_idx].edge_nodes,
            settlement_node2: Some(best_order.second_settlement),
            road_edge2: Some(best_order.second_roads[best_road2_idx].edge_nodes),
            pips: pips_first,
            pips2: Some(pips_second),
            summary,
        },
        all_sims_entries,
        red_pool_stats: HashMap::new(),
    }
}

#[cfg(feature = "stackelberg_pruning")]
fn evaluate_white12_task_dry_run(
    board: &fastcore::board::Board,
    base_state: &State,
    base_road: RoadState,
    base_army: ArmyState,
    base_seed: u64,
    task: &WhitePairTask,
    colors: &[String],
    sort_color_idx: usize,
) -> BranchResult {
    let player = task.player;
    let leader_color = colors
        .get(player as usize)
        .cloned()
        .unwrap_or_else(|| player.to_string());
    let setup_followers = white12_setup_followers(board, base_state, colors);
    let roads_a = settlement_road_options(
        board,
        base_state,
        base_road,
        base_army,
        base_seed,
        player,
        task.settlement_a,
    );
    let roads_b = settlement_road_options(
        board,
        base_state,
        base_road,
        base_army,
        base_seed,
        player,
        task.settlement_b,
    );
    let summary = dry_run_summary(colors);
    let mut all_sims_entries = Vec::new();

    let mut first_settlement = task.settlement_a;
    let mut second_settlement = task.settlement_b;
    let mut first_road = (0, 0);
    let mut second_road = (0, 0);
    let mut pips_first = task.pips_a;
    let mut pips_second = task.pips_b;

    if !roads_a.is_empty() && !roads_b.is_empty() {
        first_settlement = task.settlement_a;
        second_settlement = task.settlement_b;
        first_road = roads_a[0].edge_nodes;
        second_road = roads_b[0].edge_nodes;
        pips_first = task.pips_a;
        pips_second = task.pips_b;
        for (first_node, second_node, first_roads, second_roads) in [
            (task.settlement_a, task.settlement_b, &roads_a, &roads_b),
            (task.settlement_b, task.settlement_a, &roads_b, &roads_a),
        ] {
            for road1 in first_roads {
                for road2 in second_roads {
                    let leader_action = PlacementAction {
                        color: leader_color.clone(),
                        settlement: first_node,
                        road: road1.edge_nodes,
                    };
                    let leader_second_action = PlacementAction {
                        color: leader_color.clone(),
                        settlement: second_node,
                        road: road2.edge_nodes,
                    };
                    all_sims_entries.push(AllSimsEntry {
                        leader_branch_index: task.branch_index,
                        leader: leader_action,
                        leader_second: Some(leader_second_action),
                        followers: white12_followers_all_colors(&setup_followers),
                        summary: summary.clone(),
                        sims_run: 0,
                        source: "holdout".to_string(),
                    });
                }
            }
        }
    } else {
        let leader_action = PlacementAction {
            color: leader_color.clone(),
            settlement: first_settlement,
            road: first_road,
        };
        let leader_second_action = PlacementAction {
            color: leader_color.clone(),
            settlement: second_settlement,
            road: second_road,
        };
        all_sims_entries.push(AllSimsEntry {
            leader_branch_index: task.branch_index,
            leader: leader_action,
            leader_second: Some(leader_second_action),
            followers: white12_followers_all_colors(&setup_followers),
            summary: summary.clone(),
            sims_run: 0,
            source: "holdout".to_string(),
        });
    }

    BranchResult {
        evaluation: BranchEvaluation {
            score: summary.win_probabilities[sort_color_idx],
            branch_index: task.branch_index,
            settlement_node: first_settlement,
            road_edge: first_road,
            settlement_node2: Some(second_settlement),
            road_edge2: Some(second_road),
            pips: pips_first,
            pips2: Some(pips_second),
            summary,
        },
        all_sims_entries,
        red_pool_stats: HashMap::new(),
    }
}

#[cfg(not(feature = "stackelberg_pruning"))]
fn evaluate_white12_task(
    _board: &fastcore::board::Board,
    _base_state: &State,
    _base_road: RoadState,
    _base_army: ArmyState,
    _base_seed: u64,
    _task: &WhitePairTask,
    _seeds: &[u64],
    _workers: usize,
    _colors: &[String],
    _sort_color_idx: usize,
    _max_turns: u32,
    _stackelberg: &StackelbergConfig,
    _holdout_rerun: bool,
    _include_ts_entries: bool,
) -> BranchResult {
    panic!("white12 requires stackelberg_pruning feature");
}

#[cfg(not(feature = "stackelberg_pruning"))]
fn evaluate_white12_task_dry_run(
    _board: &fastcore::board::Board,
    _base_state: &State,
    _base_road: RoadState,
    _base_army: ArmyState,
    _base_seed: u64,
    _task: &WhitePairTask,
    _colors: &[String],
    _sort_color_idx: usize,
) -> BranchResult {
    panic!("white12 requires stackelberg_pruning feature");
}

fn evaluate_white12_tasks_dry_run(
    board: &fastcore::board::Board,
    base_state: &State,
    base_road: RoadState,
    base_army: ArmyState,
    base_seed: u64,
    tasks: &[WhitePairTask],
    colors: &[String],
    sort_color_idx: usize,
) -> Vec<BranchResult> {
    tasks
        .iter()
        .map(|task| {
            evaluate_white12_task_dry_run(
                board,
                base_state,
                base_road,
                base_army,
                base_seed,
                task,
                colors,
                sort_color_idx,
            )
        })
        .collect()
}

fn evaluate_white12_tasks(
    board: &fastcore::board::Board,
    base_state: &State,
    base_road: RoadState,
    base_army: ArmyState,
    base_seed: u64,
    tasks: &[WhitePairTask],
    seeds: &[u64],
    workers: usize,
    colors: &[String],
    sort_color_idx: usize,
    max_turns: u32,
    stackelberg: &StackelbergConfig,
    holdout_rerun: bool,
    include_ts_entries: bool,
) -> Vec<BranchResult> {
    if workers <= 1 || tasks.len() <= 1 {
        return tasks
            .iter()
            .map(|task| {
                evaluate_white12_task(
                    board,
                    base_state,
                    base_road,
                    base_army,
                    base_seed,
                    task,
                    seeds,
                    workers,
                    colors,
                    sort_color_idx,
                    max_turns,
                    stackelberg,
                    holdout_rerun,
                    include_ts_entries,
                )
            })
            .collect();
    }

    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        return tasks
            .par_iter()
            .map(|task| {
                evaluate_white12_task(
                    board,
                    base_state,
                    base_road,
                    base_army,
                    base_seed,
                    task,
                    seeds,
                    workers,
                    colors,
                    sort_color_idx,
                    max_turns,
                    stackelberg,
                    holdout_rerun,
                    include_ts_entries,
                )
            })
            .collect();
    }

    #[cfg(not(feature = "parallel"))]
    {
        tasks
            .iter()
            .map(|task| {
                evaluate_white12_task(
                    board,
                    base_state,
                    base_road,
                    base_army,
                    base_seed,
                    task,
                    seeds,
                    workers,
                    colors,
                    sort_color_idx,
                    max_turns,
                    stackelberg,
                    holdout_rerun,
                    include_ts_entries,
                )
            })
            .collect()
    }
}

fn evaluate_branch_tasks_dry_run(
    board: &fastcore::board::Board,
    base_state: &State,
    base_road: RoadState,
    base_army: ArmyState,
    base_seed: u64,
    tasks: &[BranchTask],
    colors: &[String],
    sort_color_idx: usize,
    followers: &[PlayerId],
    stackelberg: &StackelbergConfig,
) -> Vec<BranchResult> {
    tasks
        .iter()
        .map(|task| {
            evaluate_branch_task_dry_run(
                board,
                base_state,
                base_road,
                base_army,
                base_seed,
                task,
                colors,
                sort_color_idx,
                followers,
                stackelberg,
            )
        })
        .collect()
}

fn evaluate_branch_tasks(
    board: &fastcore::board::Board,
    base_state: &State,
    base_road: RoadState,
    base_army: ArmyState,
    base_seed: u64,
    tasks: &[BranchTask],
    seeds: &[u64],
    workers: usize,
    colors: &[String],
    sort_color_idx: usize,
    followers: &[PlayerId],
    max_turns: u32,
    stackelberg: &StackelbergConfig,
    holdout_rerun: bool,
    include_ts_entries: bool,
) -> Vec<BranchResult> {
    #[cfg(feature = "stackelberg_pruning")]
    {
        // To pool red priors across leader roads that share the same leader settlement,
        // we must evaluate those tasks sequentially within each settlement group.
        if followers.len() == 2 && tasks.len() > 1 {
            let mut ordered: Vec<&BranchTask> = tasks.iter().collect();
            ordered.sort_by(|a, b| {
                a.settlement_node
                    .cmp(&b.settlement_node)
                    .then_with(|| a.branch_index.cmp(&b.branch_index))
            });

            let mut results_by_branch: HashMap<usize, BranchResult> =
                HashMap::with_capacity(tasks.len());

            if USE_GLOBAL_RED_POOLING {
                let mut global_pool: RedPooledCounts = HashMap::new();
                for task in ordered {
                    let result = evaluate_branch_task(
                        board,
                        base_state,
                        base_road,
                        base_army,
                        base_seed,
                        task,
                        seeds,
                        workers,
                        colors,
                        sort_color_idx,
                        followers,
                        max_turns,
                        stackelberg,
                        holdout_rerun,
                        include_ts_entries,
                        Some(&global_pool),
                    );
                    merge_red_pool_counts(&mut global_pool, &result.red_pool_stats);
                    results_by_branch.insert(task.branch_index, result);
                }
            } else {
                let mut pools_by_settlement: HashMap<NodeId, RedPooledCounts> = HashMap::new();
                for task in ordered {
                    let pooled_snapshot = pools_by_settlement.get(&task.settlement_node).cloned();
                    let result = evaluate_branch_task(
                        board,
                        base_state,
                        base_road,
                        base_army,
                        base_seed,
                        task,
                        seeds,
                        workers,
                        colors,
                        sort_color_idx,
                        followers,
                        max_turns,
                        stackelberg,
                        holdout_rerun,
                        include_ts_entries,
                        pooled_snapshot.as_ref(),
                    );
                    let pool_entry = pools_by_settlement
                        .entry(task.settlement_node)
                        .or_insert_with(HashMap::new);
                    merge_red_pool_counts(pool_entry, &result.red_pool_stats);
                    results_by_branch.insert(task.branch_index, result);
                }
            }

            return tasks
                .iter()
                .map(|task| {
                    results_by_branch
                        .remove(&task.branch_index)
                        .expect("missing pooled branch result")
                })
                .collect();
        }
    }

    if workers <= 1 || tasks.len() <= 1 {
        return tasks
            .iter()
            .map(|task| {
                evaluate_branch_task(
                    board,
                    base_state,
                    base_road,
                    base_army,
                    base_seed,
                    task,
                    seeds,
                    workers,
                    colors,
                    sort_color_idx,
                    followers,
                    max_turns,
                    stackelberg,
                    holdout_rerun,
                    include_ts_entries,
                    None,
                )
            })
            .collect();
    }

    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        return tasks
            .par_iter()
            .map(|task| {
                evaluate_branch_task(
                    board,
                    base_state,
                    base_road,
                    base_army,
                    base_seed,
                    task,
                    seeds,
                    workers,
                    colors,
                    sort_color_idx,
                    followers,
                    max_turns,
                    stackelberg,
                    holdout_rerun,
                    include_ts_entries,
                    None,
                )
            })
            .collect();
    }

    #[cfg(not(feature = "parallel"))]
    {
        tasks
            .iter()
            .map(|task| {
                evaluate_branch_task(
                    board,
                    base_state,
                    base_road,
                    base_army,
                    base_seed,
                    task,
                    seeds,
                    workers,
                    colors,
                    sort_color_idx,
                    followers,
                    max_turns,
                    stackelberg,
                    holdout_rerun,
                    include_ts_entries,
                    None,
                )
            })
            .collect()
    }
}

fn dense_ranks(sorted: &[BranchEvaluation]) -> Vec<(usize, BranchEvaluation)> {
    let mut ranks = Vec::with_capacity(sorted.len());
    let mut last_score = None;
    let mut current_rank = 0usize;
    for evaluation in sorted.iter() {
        if last_score.is_none() || evaluation.score < last_score.unwrap() {
            current_rank += 1;
            last_score = Some(evaluation.score);
        }
        ranks.push((current_rank, evaluation.clone()));
    }
    ranks
}

fn csv_headers(colors: &[String], white12: bool) -> Vec<String> {
    let mut headers = vec![
        "TIMESTAMP_UTC".to_string(),
        "HOST_CPU_COUNT".to_string(),
        "BASE_SEED".to_string(),
        "START_SEED".to_string(),
        "NUM_SIMS".to_string(),
        "SORT_COLOR".to_string(),
        "WORKERS_USED".to_string(),
        "BRANCH_INDEX".to_string(),
        "RANK".to_string(),
        "SETTLEMENT".to_string(),
        "ROAD".to_string(),
    ];
    if white12 {
        headers.push("SETTLEMENT2".to_string());
        headers.push("ROAD2".to_string());
    }
    headers.extend_from_slice(&[
        "WOOD_PIPS".to_string(),
        "BRICK_PIPS".to_string(),
        "SHEEP_PIPS".to_string(),
        "WHEAT_PIPS".to_string(),
        "ORE_PIPS".to_string(),
    ]);
    if white12 {
        headers.extend_from_slice(&[
            "WOOD_PIPS2".to_string(),
            "BRICK_PIPS2".to_string(),
            "SHEEP_PIPS2".to_string(),
            "WHEAT_PIPS2".to_string(),
            "ORE_PIPS2".to_string(),
        ]);
    }
    headers.extend_from_slice(&[
        "AVG_TURNS".to_string(),
        "WALL_TIME_SEC".to_string(),
        "CPU_TIME_SEC".to_string(),
        "WINNER".to_string(),
    ]);
    for color in colors {
        headers.push(format!("WIN_{color}"));
    }
    for color in colors {
        headers.push(format!("AVG_VP_{color}"));
    }
    headers
}

#[cfg(test)]
mod playout_aggregation_tests {
    use super::*;

    fn legacy_win_probabilities(results: &[PlayoutResult]) -> [f64; PLAYER_COUNT] {
        let mut counts = [0u32; PLAYER_COUNT];
        let mut total = 0u32;
        for result in results {
            if let Some(winner) = result.winner {
                counts[winner as usize] += 1;
                total += 1;
            }
        }
        if total == 0 {
            return [0.0; PLAYER_COUNT];
        }
        let mut probs = [0.0; PLAYER_COUNT];
        for idx in 0..PLAYER_COUNT {
            probs[idx] = ((counts[idx] as f64 / total as f64) * 100.0 * 10.0).round() / 10.0;
        }
        probs
    }

    fn legacy_average_vps(results: &[PlayoutResult]) -> [f64; PLAYER_COUNT] {
        if results.is_empty() {
            return [0.0; PLAYER_COUNT];
        }
        let mut totals = [0u32; PLAYER_COUNT];
        for result in results {
            for idx in 0..PLAYER_COUNT {
                totals[idx] += result.vps_by_player[idx] as u32;
            }
        }
        let mut avg = [0.0; PLAYER_COUNT];
        let count = results.len() as f64;
        for idx in 0..PLAYER_COUNT {
            avg[idx] = (totals[idx] as f64 / count * 100.0).round() / 100.0;
        }
        avg
    }

    fn legacy_average_turns(results: &[PlayoutResult]) -> f64 {
        if results.is_empty() {
            return 0.0;
        }
        let total: u64 = results.iter().map(|r| r.num_turns as u64).sum();
        let avg = total as f64 / results.len() as f64;
        (avg * 100.0).round() / 100.0
    }

    fn synthetic_results() -> Vec<PlayoutResult> {
        vec![
            PlayoutResult {
                winner: Some(0),
                vps_by_player: [10, 8, 6, 4],
                num_turns: 50,
            },
            PlayoutResult {
                winner: Some(2),
                vps_by_player: [7, 6, 10, 5],
                num_turns: 40,
            },
            PlayoutResult {
                winner: None,
                vps_by_player: [9, 7, 9, 6],
                num_turns: 60,
            },
            PlayoutResult {
                winner: Some(2),
                vps_by_player: [6, 5, 10, 8],
                num_turns: 45,
            },
        ]
    }

    #[test]
    fn playout_aggregate_matches_legacy_reducers() {
        let results = synthetic_results();
        let mut aggregate = PlayoutAggregate::default();
        for result in &results {
            aggregate.update(result);
        }

        assert_eq!(
            win_probabilities_from_counts(&aggregate.wins, aggregate.total_non_none),
            legacy_win_probabilities(&results)
        );
        assert_eq!(
            average_vps_from_totals(&aggregate.total_vps, aggregate.samples),
            legacy_average_vps(&results)
        );
        assert_eq!(
            average_turns_from_total(aggregate.total_turns, aggregate.samples),
            legacy_average_turns(&results)
        );
    }

    #[test]
    fn playout_aggregate_merge_matches_single_pass() {
        let results = synthetic_results();

        let mut full = PlayoutAggregate::default();
        for result in &results {
            full.update(result);
        }

        let mut left = PlayoutAggregate::default();
        for result in &results[..2] {
            left.update(result);
        }
        let mut right = PlayoutAggregate::default();
        for result in &results[2..] {
            right.update(result);
        }
        left.merge(&right);

        assert_eq!(left.samples, full.samples);
        assert_eq!(left.total_non_none, full.total_non_none);
        assert_eq!(left.total_turns, full.total_turns);
        assert_eq!(left.wins, full.wins);
        assert_eq!(left.total_vps, full.total_vps);
    }
}

#[cfg(all(test, feature = "stackelberg_pruning"))]
mod tests {
    use super::*;

    #[test]
    fn stackelberg_seed_deterministic() {
        let s1 = stackelberg_seed(42, 1, 2, 3);
        let s2 = stackelberg_seed(42, 1, 2, 3);
        let s3 = stackelberg_seed(42, 1, 2, 4);
        assert_eq!(s1, s2);
        assert_ne!(s1, s3);
    }

    #[test]
    fn top_two_indices_breaks_ties_by_index() {
        let values = vec![0.5, 0.5, 0.4];
        let (a, b) = top_two_indices(&values);
        assert_eq!(a, 0);
        assert_eq!(b, 1);
    }

    #[test]
    fn leaf_stats_update_tracks_samples() {
        let mut stats = LeafStats::new(1.0, 1.0);
        let result = PlayoutResult {
            winner: Some(1),
            vps_by_player: [0, 10, 0, 0],
            num_turns: 30,
        };
        stats.update(&result, 1, Some(2));
        assert_eq!(stats.samples, 1);
        assert_eq!(stats.alpha_red, 2.0);
        assert_eq!(stats.beta_red, 1.0);
        assert_eq!(stats.alpha_blue, 1.0);
        assert_eq!(stats.beta_blue, 2.0);
        assert_eq!(stats.wins[1], 1);
        assert_eq!(stats.total_vps[1], 10);
        assert_eq!(stats.total_turns, 30);
    }
}

fn all_sims_headers(colors: &[String]) -> Vec<String> {
    let mut headers = vec![
        "TIMESTAMP_UTC".to_string(),
        "HOST_CPU_COUNT".to_string(),
        "BASE_SEED".to_string(),
        "START_SEED".to_string(),
        "NUM_SIMS".to_string(),
        "SIMS_RUN".to_string(),
        "SOURCE".to_string(),
        "WORKERS_USED".to_string(),
        "LEADER_BRANCH_INDEX".to_string(),
        "LEADER_COLOR".to_string(),
        "LEADER_SETTLEMENT".to_string(),
        "LEADER_ROAD".to_string(),
        "LEADER_SETTLEMENT2".to_string(),
        "LEADER_ROAD2".to_string(),
        "FOLLOWER1_COLOR".to_string(),
        "FOLLOWER1_SETTLEMENT".to_string(),
        "FOLLOWER1_ROAD".to_string(),
        "FOLLOWER2_COLOR".to_string(),
        "FOLLOWER2_SETTLEMENT".to_string(),
        "FOLLOWER2_ROAD".to_string(),
        "FOLLOWER3_COLOR".to_string(),
        "FOLLOWER3_SETTLEMENT".to_string(),
        "FOLLOWER3_ROAD".to_string(),
        "AVG_TURNS".to_string(),
        "WALL_TIME_SEC".to_string(),
        "CPU_TIME_SEC".to_string(),
        "WINNER".to_string(),
    ];
    for color in colors {
        headers.push(format!("WIN_{color}"));
    }
    for color in colors {
        headers.push(format!("AVG_VP_{color}"));
    }
    headers
}

fn branch_row(
    evaluation: &BranchEvaluation,
    rank: usize,
    colors: &[String],
    context: &RunContext,
) -> Vec<String> {
    let edge = evaluation.road_edge;
    let pips = evaluation.pips;
    let summary = &evaluation.summary;
    let mut row = vec![
        context.timestamp_utc.clone(),
        context.host_cpu_count.to_string(),
        context.base_seed.to_string(),
        context.start_seed.to_string(),
        context.num_sims.to_string(),
        context.sort_color.clone(),
        summary.workers_used.to_string(),
        evaluation.branch_index.to_string(),
        rank.to_string(),
        evaluation.settlement_node.to_string(),
        format!("{}-{}", edge.0, edge.1),
    ];
    if context.white12 {
        row.push(
            evaluation
                .settlement_node2
                .map(|node| node.to_string())
                .unwrap_or_default(),
        );
        row.push(
            evaluation
                .road_edge2
                .map(|edge| format!("{}-{}", edge.0, edge.1))
                .unwrap_or_default(),
        );
    }
    row.extend_from_slice(&[
        pips[0].to_string(),
        pips[1].to_string(),
        pips[2].to_string(),
        pips[3].to_string(),
        pips[4].to_string(),
    ]);
    if context.white12 {
        let pips2 = evaluation.pips2.unwrap_or([0u32; RESOURCE_COUNT]);
        row.extend_from_slice(&[
            pips2[0].to_string(),
            pips2[1].to_string(),
            pips2[2].to_string(),
            pips2[3].to_string(),
            pips2[4].to_string(),
        ]);
    }
    row.extend_from_slice(&[
        format!("{:.2}", summary.avg_turns),
        format!("{:.4}", summary.wall_time_sec),
        format!("{:.4}", summary.cpu_time_sec),
        summary.winner_label.clone(),
    ]);
    for (idx, _color) in colors.iter().enumerate() {
        row.push(format!("{:.1}", summary.win_probabilities[idx]));
    }
    for (idx, _color) in colors.iter().enumerate() {
        row.push(format!("{:.2}", summary.avg_vps_by_player[idx]));
    }
    row
}

fn all_sims_row(entry: &AllSimsEntry, colors: &[String], context: &RunContext) -> Vec<String> {
    let summary = &entry.summary;
    let leader_second = entry.leader_second.as_ref();
    let follower1 = entry.followers.get(0);
    let follower2 = entry.followers.get(1);
    let follower3 = entry.followers.get(2);

    let mut row = vec![
        context.timestamp_utc.clone(),
        context.host_cpu_count.to_string(),
        context.base_seed.to_string(),
        context.start_seed.to_string(),
        context.num_sims.to_string(),
        entry.sims_run.to_string(),
        entry.source.clone(),
        summary.workers_used.to_string(),
        entry.leader_branch_index.to_string(),
        entry.leader.color.clone(),
        entry.leader.settlement.to_string(),
        format!("{}-{}", entry.leader.road.0, entry.leader.road.1),
        leader_second
            .map(|action| action.settlement.to_string())
            .unwrap_or_default(),
        leader_second
            .map(|action| format!("{}-{}", action.road.0, action.road.1))
            .unwrap_or_default(),
        follower1
            .map(|action| action.color.clone())
            .unwrap_or_default(),
        follower1
            .map(|action| action.settlement.to_string())
            .unwrap_or_default(),
        follower1
            .map(|action| format!("{}-{}", action.road.0, action.road.1))
            .unwrap_or_default(),
        follower2
            .map(|action| action.color.clone())
            .unwrap_or_default(),
        follower2
            .map(|action| action.settlement.to_string())
            .unwrap_or_default(),
        follower2
            .map(|action| format!("{}-{}", action.road.0, action.road.1))
            .unwrap_or_default(),
        follower3
            .map(|action| action.color.clone())
            .unwrap_or_default(),
        follower3
            .map(|action| action.settlement.to_string())
            .unwrap_or_default(),
        follower3
            .map(|action| format!("{}-{}", action.road.0, action.road.1))
            .unwrap_or_default(),
        format!("{:.2}", summary.avg_turns),
        format!("{:.4}", summary.wall_time_sec),
        format!("{:.4}", summary.cpu_time_sec),
        summary.winner_label.clone(),
    ];
    for (idx, _color) in colors.iter().enumerate() {
        row.push(format!("{:.1}", summary.win_probabilities[idx]));
    }
    for (idx, _color) in colors.iter().enumerate() {
        row.push(format!("{:.2}", summary.avg_vps_by_player[idx]));
    }
    row
}

fn action_sort_key(action: Option<&PlacementAction>) -> (String, NodeId, NodeId, NodeId) {
    if let Some(action) = action {
        (
            action.color.clone(),
            action.settlement,
            action.road.0,
            action.road.1,
        )
    } else {
        (String::new(), 0, 0, 0)
    }
}

fn sort_all_sims_entries(entries: &mut Vec<AllSimsEntry>) {
    entries.sort_by(|a, b| {
        let a_leader2 = action_sort_key(a.leader_second.as_ref());
        let b_leader2 = action_sort_key(b.leader_second.as_ref());
        let a_key1 = action_sort_key(a.followers.get(0));
        let b_key1 = action_sort_key(b.followers.get(0));
        let a_key2 = action_sort_key(a.followers.get(1));
        let b_key2 = action_sort_key(b.followers.get(1));
        let a_key3 = action_sort_key(a.followers.get(2));
        let b_key3 = action_sort_key(b.followers.get(2));
        a.leader_branch_index
            .cmp(&b.leader_branch_index)
            .then_with(|| {
                (a.leader.settlement, a.leader.road.0, a.leader.road.1).cmp(&(
                    b.leader.settlement,
                    b.leader.road.0,
                    b.leader.road.1,
                ))
            })
            .then_with(|| a_leader2.cmp(&b_leader2))
            .then_with(|| a_key1.cmp(&b_key1))
            .then_with(|| a_key2.cmp(&b_key2))
            .then_with(|| a_key3.cmp(&b_key3))
    });
}

#[derive(Clone, Debug)]
struct RunContext {
    timestamp_utc: String,
    host_cpu_count: usize,
    base_seed: u64,
    start_seed: u64,
    num_sims: u32,
    sort_color: String,
    white12: bool,
}

fn utc_timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{seconds}")
}

fn main() {
    let mut args = parse_args();
    let (actions, colors, current_color) = load_actions(&args.state_path);
    if !args.blue2 && !args.orange2 && !args.white12 {
        if let Some(color) = current_color.as_deref() {
            match color {
                "BLUE" => args.blue2 = true,
                "ORANGE" => args.orange2 = true,
                "WHITE" => args.white12 = true,
                _ => {}
            }
        }
    }
    let mode_count = [args.blue2, args.orange2, args.white12]
        .iter()
        .filter(|&&mode| mode)
        .count();
    if mode_count > 1 {
        eprintln!("--blue2, --orange2, and --white12 are mutually exclusive");
        std::process::exit(2);
    }
    #[cfg(not(feature = "stackelberg_pruning"))]
    if args.white12 {
        eprintln!("--white12 requires the stackelberg_pruning feature");
        std::process::exit(2);
    }
    let actions = if args.blue2 {
        truncate_before_color2(&actions, "BLUE")
    } else if args.orange2 {
        truncate_before_color2(&actions, "ORANGE")
    } else if args.white12 {
        truncate_before_color1(&actions, "WHITE")
    } else {
        actions
    };
    let board = board_from_json(&args.board_path).expect("failed to load board json");
    let (base_state, base_road, base_army) =
        apply_action_history(&board, &actions, &colors, args.seed);

    let followers: Vec<PlayerId> = if args.blue2 {
        vec![color_to_player(&colors, "RED")]
    } else if args.orange2 {
        vec![
            color_to_player(&colors, "BLUE"),
            color_to_player(&colors, "RED"),
        ]
    } else {
        Vec::new()
    };
    let is_stackelberg = !followers.is_empty() || args.white12;
    let include_ts_entries = !args.holdout_only;
    let all_sims_output = if is_stackelberg {
        Some(
            args.all_sims_output
                .clone()
                .unwrap_or_else(|| default_all_sims_output(&args.output)),
        )
    } else {
        None
    };
    let sort_color = args
        .sort_color
        .clone()
        .unwrap_or_else(|| colors.get(0).cloned().unwrap_or_else(|| "RED".to_string()));
    let sort_color_idx = colors.iter().position(|c| c == &sort_color).unwrap_or(0);

    let white_player = if args.white12 {
        Some(color_to_player(&colors, "WHITE"))
    } else {
        None
    };
    let stackelberg_config = StackelbergConfig {
        budget: args.budget.max(1),
        alpha0: args.alpha0.max(1e-6),
        beta0: args.beta0.max(1e-6),
        red_global_prior_weight: args.red_global_prior_weight.max(0.0),
        rho: args.rho.clamp(0.0, 1.0),
        min_samples: args.min_samples.max(0),
        batch_sims: {
            let mut batch = if args.batch_sims == 0 {
                args.workers.max(1)
            } else {
                args.batch_sims
            };
            if args.workers > 1 && batch < 2 {
                batch = 2;
            }
            batch
        },
    };
    let seeds: Vec<u64> = (0..args.num_sims)
        .map(|offset| args.start_seed + offset as u64)
        .collect();
    if args.workers > 1 && !args.dry_run {
        init_global_pool(args.workers);
    }

    let results = if let Some(player) = white_player {
        let white_tasks = build_white12_tasks(
            &board,
            &base_state,
            base_road,
            base_army,
            args.seed,
            args.limit,
            args.leader_settlement,
            player,
        );
        if args.dry_run {
            evaluate_white12_tasks_dry_run(
                &board,
                &base_state,
                base_road,
                base_army,
                args.seed,
                &white_tasks,
                &colors,
                sort_color_idx,
            )
        } else {
            evaluate_white12_tasks(
                &board,
                &base_state,
                base_road,
                base_army,
                args.seed,
                &white_tasks,
                &seeds,
                args.workers,
                &colors,
                sort_color_idx,
                args.max_turns,
                &stackelberg_config,
                args.holdout_rerun,
                include_ts_entries,
            )
        }
    } else {
        let tasks = build_branch_tasks(
            &board,
            &base_state,
            base_road,
            base_army,
            args.seed,
            args.limit,
            args.leader_settlement,
        );
        if args.dry_run {
            evaluate_branch_tasks_dry_run(
                &board,
                &base_state,
                base_road,
                base_army,
                args.seed,
                &tasks,
                &colors,
                sort_color_idx,
                &followers,
                &stackelberg_config,
            )
        } else {
            evaluate_branch_tasks(
                &board,
                &base_state,
                base_road,
                base_army,
                args.seed,
                &tasks,
                &seeds,
                args.workers,
                &colors,
                sort_color_idx,
                &followers,
                args.max_turns,
                &stackelberg_config,
                args.holdout_rerun,
                include_ts_entries,
            )
        }
    };

    let mut branches = Vec::with_capacity(results.len());
    let mut all_sims_entries = Vec::new();
    for result in results {
        branches.push(result.evaluation);
        all_sims_entries.extend(result.all_sims_entries);
    }

    branches.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.branch_index.cmp(&b.branch_index))
    });

    let ranked = dense_ranks(&branches);
    let context = RunContext {
        timestamp_utc: utc_timestamp(),
        host_cpu_count: available_workers(),
        base_seed: args.seed,
        start_seed: args.start_seed,
        num_sims: args.num_sims,
        sort_color,
        white12: args.white12,
    };

    ensure_parent_dir(&args.output);
    let mut output = File::create(&args.output).expect("failed to create output csv");
    let headers = csv_headers(&colors, args.white12);
    writeln!(output, "{}", headers.join(",")).expect("failed to write headers");
    for (rank, evaluation) in ranked {
        let row = branch_row(&evaluation, rank, &colors, &context);
        writeln!(output, "{}", row.join(",")).expect("failed to write row");
    }

    if let Some(all_sims_path) = all_sims_output {
        let (ts_path, holdout_path) = split_all_sims_outputs(&all_sims_path);
        let mut holdout_entries = Vec::new();
        let headers = all_sims_headers(&colors);
        if args.holdout_only {
            for entry in all_sims_entries.drain(..) {
                if entry.source != "ts" {
                    holdout_entries.push(entry);
                }
            }
        } else {
            let mut ts_entries = Vec::new();
            for entry in all_sims_entries.drain(..) {
                if entry.source == "ts" {
                    ts_entries.push(entry);
                } else {
                    holdout_entries.push(entry);
                }
            }
            sort_all_sims_entries(&mut ts_entries);
            ensure_parent_dir(&ts_path);
            let mut ts_output =
                File::create(ts_path).expect("failed to create all sims ts output csv");
            writeln!(ts_output, "{}", headers.join(",")).expect("failed to write headers");
            for entry in &ts_entries {
                let row = all_sims_row(entry, &colors, &context);
                writeln!(ts_output, "{}", row.join(",")).expect("failed to write row");
            }
        }

        sort_all_sims_entries(&mut holdout_entries);
        ensure_parent_dir(&holdout_path);
        let mut holdout_output =
            File::create(holdout_path).expect("failed to create all sims holdout output csv");
        writeln!(holdout_output, "{}", headers.join(",")).expect("failed to write headers");
        for entry in &holdout_entries {
            let row = all_sims_row(entry, &colors, &context);
            writeln!(holdout_output, "{}", row.join(",")).expect("failed to write row");
        }
    }
}
