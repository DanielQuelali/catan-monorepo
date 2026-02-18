use fastcore::{simulate_many, SimConfig};
use std::env;

fn print_usage() {
    eprintln!(
        "Usage: smoke [--seeds <csv>] [--max-turns <u16>]\n  Example: smoke --seeds 1,2,3 --max-turns 200"
    );
}

fn parse_seeds(raw: &str) -> Result<Vec<u64>, String> {
    let mut seeds = Vec::new();
    for chunk in raw.split(',') {
        let trimmed = chunk.trim();
        if trimmed.is_empty() {
            continue;
        }
        let seed = trimmed
            .parse::<u64>()
            .map_err(|_| format!("Invalid seed value: {trimmed}"))?;
        seeds.push(seed);
    }
    if seeds.is_empty() {
        return Err("No seeds provided.".to_string());
    }
    Ok(seeds)
}

fn default_seeds() -> Vec<u64> {
    (1u64..=10u64).collect()
}

fn main() {
    let mut seeds: Option<Vec<u64>> = None;
    let mut max_turns: Option<u16> = None;
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seeds" => {
                let value = match args.next() {
                    Some(value) => value,
                    None => {
                        eprintln!("Missing value for --seeds.");
                        print_usage();
                        std::process::exit(2);
                    }
                };
                match parse_seeds(&value) {
                    Ok(parsed) => seeds = Some(parsed),
                    Err(message) => {
                        eprintln!("{message}");
                        print_usage();
                        std::process::exit(2);
                    }
                }
            }
            "--max-turns" => {
                let value = match args.next() {
                    Some(value) => value,
                    None => {
                        eprintln!("Missing value for --max-turns.");
                        print_usage();
                        std::process::exit(2);
                    }
                };
                match value.parse::<u16>() {
                    Ok(parsed) => max_turns = Some(parsed),
                    Err(_) => {
                        eprintln!("Invalid value for --max-turns: {value}");
                        print_usage();
                        std::process::exit(2);
                    }
                }
            }
            "-h" | "--help" => {
                print_usage();
                return;
            }
            _ => {
                if let Some(value) = arg.strip_prefix("--seeds=") {
                    match parse_seeds(value) {
                        Ok(parsed) => seeds = Some(parsed),
                        Err(message) => {
                            eprintln!("{message}");
                            print_usage();
                            std::process::exit(2);
                        }
                    }
                } else if let Some(value) = arg.strip_prefix("--max-turns=") {
                    match value.parse::<u16>() {
                        Ok(parsed) => max_turns = Some(parsed),
                        Err(_) => {
                            eprintln!("Invalid value for --max-turns: {value}");
                            print_usage();
                            std::process::exit(2);
                        }
                    }
                } else {
                    eprintln!("Unknown argument: {arg}");
                    print_usage();
                    std::process::exit(2);
                }
            }
        }
    }

    let seeds = seeds.unwrap_or_else(default_seeds);
    let mut config = SimConfig::default();
    if let Some(max_turns) = max_turns {
        config.max_turns = max_turns;
    }

    let stats = simulate_many(&seeds, &config);
    println!(
        "games={} turns={} wins={},{},{},{} illegal_actions={}",
        stats.games,
        stats.turns,
        stats.wins[0],
        stats.wins[1],
        stats.wins[2],
        stats.wins[3],
        stats.illegal_actions
    );
}
