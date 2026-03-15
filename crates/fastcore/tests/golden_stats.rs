use fastcore::{simulate_many, simulate_policy_log, SimConfig, Stats};
use std::fs;

fn parse_stats(line: &str) -> Stats {
    let mut stats = Stats::default();
    for part in line.split_whitespace() {
        let mut iter = part.split('=');
        let key = iter.next().unwrap_or("");
        let value = iter.next().unwrap_or("");
        match key {
            "games" => stats.games = value.parse().unwrap(),
            "turns" => stats.turns = value.parse().unwrap(),
            "wins" => {
                let wins: Vec<u64> = value
                    .split(',')
                    .map(|item| item.parse::<u64>().unwrap())
                    .collect();
                stats.wins.copy_from_slice(&wins);
            }
            "illegal_actions" => stats.illegal_actions = value.parse().unwrap(),
            _ => {}
        }
    }
    stats
}

#[test]
fn golden_seed_stats_match() {
    let seeds_text = fs::read_to_string("tests/data/golden_seeds.txt").unwrap();
    let seeds: Vec<u64> = seeds_text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.parse().unwrap())
        .collect();

    let expected_path = if cfg!(feature = "legacy_robber") {
        "tests/data/golden_stats_legacy.txt"
    } else {
        "tests/data/golden_stats.txt"
    };
    let expected_line = fs::read_to_string(expected_path).unwrap();
    let expected = parse_stats(expected_line.trim());

    let actual = simulate_many(&seeds, &SimConfig::default());

    assert_eq!(actual.games, expected.games);
    assert_eq!(actual.turns, expected.turns);
    assert_eq!(actual.wins, expected.wins);
    assert_eq!(actual.illegal_actions, expected.illegal_actions);
}

#[test]
fn policy_logs_are_deterministic_for_fixed_seeds() {
    let seeds = [1000u64, 1001u64, 1002u64, 1003u64];
    let config = SimConfig::default();

    let first = simulate_policy_log(&seeds, &config);
    let second = simulate_policy_log(&seeds, &config);

    assert_eq!(first, second);
}
