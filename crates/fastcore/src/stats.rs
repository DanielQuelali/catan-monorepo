use crate::types::PLAYER_COUNT;

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub games: u64,
    pub turns: u64,
    pub wins: [u64; PLAYER_COUNT],
    pub illegal_actions: u64,
}

impl Stats {
    pub fn merge(&mut self, other: &Stats) {
        self.games += other.games;
        self.turns += other.turns;
        self.illegal_actions += other.illegal_actions;
        for i in 0..PLAYER_COUNT {
            self.wins[i] += other.wins[i];
        }
    }
}

pub fn merge_worker_stats(mut workers: Vec<(u64, Stats)>) -> Stats {
    workers.sort_by_key(|(worker_id, _)| *worker_id);
    let mut merged = Stats::default();
    for (_, stats) in workers {
        merged.merge(&stats);
    }
    merged
}

#[derive(Clone, Debug, Default)]
pub struct EvalStats {
    pub games: u64,
    pub score_sum: f64,
}

impl EvalStats {
    pub fn merge(&mut self, other: &EvalStats) {
        self.games += other.games;
        self.score_sum += other.score_sum;
    }
}
