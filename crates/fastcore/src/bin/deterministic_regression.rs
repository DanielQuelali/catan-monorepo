use fastcore::{simulate_many, simulate_policy_log, SimConfig, Stats, PLAYER_COUNT};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::PathBuf;

const DEFAULT_SEED_START: u64 = 1000;
const DEFAULT_SEED_COUNT: usize = 64;
const DEFAULT_MAX_TURNS: u16 = 2000;

#[derive(Debug)]
struct Args {
    seed_start: u64,
    seed_count: usize,
    max_turns: u16,
    seeds_file: Option<PathBuf>,
    out: Option<PathBuf>,
}

#[derive(Serialize)]
struct RegressionReport {
    format: &'static str,
    seed_start: Option<u64>,
    seed_count: usize,
    max_turns: u16,
    aggregate: AggregateStats,
    per_seed: Vec<SeedResult>,
}

#[derive(Serialize)]
struct AggregateStats {
    games: u64,
    turns: u64,
    wins: [u64; PLAYER_COUNT],
    illegal_actions: u64,
}

#[derive(Serialize)]
struct SeedResult {
    seed: u64,
    winner: Option<u8>,
    turns: u64,
    illegal_actions: u64,
    trace_hash: String,
}

fn usage() -> &'static str {
    "Usage: deterministic_regression [--seed-start <u64>] [--seed-count <usize>] [--max-turns <u16>] [--seeds-file <path>] [--out <path>]"
}

fn parse_args() -> Result<Args, String> {
    let mut seed_start = DEFAULT_SEED_START;
    let mut seed_count = DEFAULT_SEED_COUNT;
    let mut max_turns = DEFAULT_MAX_TURNS;
    let mut seeds_file = None;
    let mut out = None;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seed-start" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --seed-start".to_string())?;
                seed_start = value
                    .parse()
                    .map_err(|_| format!("Invalid --seed-start value: {value}"))?;
            }
            "--seed-count" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --seed-count".to_string())?;
                seed_count = value
                    .parse()
                    .map_err(|_| format!("Invalid --seed-count value: {value}"))?;
            }
            "--max-turns" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --max-turns".to_string())?;
                max_turns = value
                    .parse()
                    .map_err(|_| format!("Invalid --max-turns value: {value}"))?;
            }
            "--seeds-file" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --seeds-file".to_string())?;
                seeds_file = Some(PathBuf::from(value));
            }
            "--out" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --out".to_string())?;
                out = Some(PathBuf::from(value));
            }
            "-h" | "--help" => {
                return Err(usage().to_string());
            }
            _ => return Err(format!("Unknown arg: {arg}\n{}", usage())),
        }
    }

    Ok(Args {
        seed_start,
        seed_count,
        max_turns,
        seeds_file,
        out,
    })
}

fn seeds_from_args(args: &Args) -> Result<Vec<u64>, String> {
    if let Some(path) = &args.seeds_file {
        let text = fs::read_to_string(path)
            .map_err(|err| format!("Failed to read seeds file {}: {err}", path.display()))?;
        let mut seeds = Vec::new();
        for (line_no, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let seed = trimmed.parse::<u64>().map_err(|_| {
                format!(
                    "Invalid seed in {} at line {}: {}",
                    path.display(),
                    line_no + 1,
                    trimmed
                )
            })?;
            seeds.push(seed);
        }
        if seeds.is_empty() {
            return Err(format!("Seeds file {} has no seeds", path.display()));
        }
        return Ok(seeds);
    }

    Ok((0..args.seed_count)
        .map(|offset| args.seed_start + offset as u64)
        .collect())
}

fn stats_winner(stats: &Stats) -> Option<u8> {
    stats
        .wins
        .iter()
        .enumerate()
        .find_map(|(idx, wins)| (*wins > 0).then_some(idx as u8))
}

fn fnv1a_64_hex(lines: &[String]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for line in lines {
        for byte in line.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= b'\n' as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    let seeds = match seeds_from_args(&args) {
        Ok(seeds) => seeds,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let config = SimConfig {
        max_turns: args.max_turns,
    };

    let aggregate = simulate_many(&seeds, &config);
    let mut per_seed = Vec::with_capacity(seeds.len());
    for seed in &seeds {
        let seed_only = [*seed];
        let seed_stats = simulate_many(&seed_only, &config);
        let logs = simulate_policy_log(&seed_only, &config);
        let trace_hash = logs
            .first()
            .map(|entries| fnv1a_64_hex(entries))
            .unwrap_or_else(|| fnv1a_64_hex(&[]));
        per_seed.push(SeedResult {
            seed: *seed,
            winner: stats_winner(&seed_stats),
            turns: seed_stats.turns,
            illegal_actions: seed_stats.illegal_actions,
            trace_hash,
        });
    }

    let report = RegressionReport {
        format: "fastcore-single-thread-regression-v1",
        seed_start: args.seeds_file.is_none().then_some(args.seed_start),
        seed_count: seeds.len(),
        max_turns: args.max_turns,
        aggregate: AggregateStats {
            games: aggregate.games,
            turns: aggregate.turns,
            wins: aggregate.wins,
            illegal_actions: aggregate.illegal_actions,
        },
        per_seed,
    };

    let encoded = match serde_json::to_string_pretty(&report) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("Failed to encode report JSON: {err}");
            std::process::exit(1);
        }
    };

    if let Some(path) = args.out {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(err) = fs::create_dir_all(parent) {
                    eprintln!(
                        "Failed to create output directory {}: {err}",
                        parent.display()
                    );
                    std::process::exit(1);
                }
            }
        }
        if let Err(err) = fs::write(&path, encoded) {
            eprintln!("Failed to write report {}: {err}", path.display());
            std::process::exit(1);
        }
    } else {
        println!("{encoded}");
    }
}
