#![forbid(unsafe_code)]

pub mod actions;
pub mod board;
pub mod board_config;
mod board_data;
pub mod delta;
pub mod engine;
pub mod rng;
pub mod rules;
pub mod state;
pub mod stats;
pub mod types;
pub mod value_player;

pub use actions::{ActionCode, ActionKind};
pub use board::{Board, STANDARD_BOARD};
pub use engine::{evaluate_many, simulate_many, simulate_policy_log, SimConfig};
pub use state::State;
pub use stats::{EvalStats, Stats};
pub use types::*;
