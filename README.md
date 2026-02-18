# catan-monorepo

Clean monorepo containing:

- `crates/fastcore`: Rust fastcore library extracted from `catan-sim/fastcore`
- `crates/initial-branch-analysis`: Rust analysis binary separated from `fastcore`
- `apps/catanatron-ui`: UI extracted from `catan-sim/catanatron/ui`
- `apps/hex-gambit`: standalone Hex Gambit playstyle evaluator web app

Run Hex Gambit from repo root:

`./run-hex-gambit.sh`

This launcher always frees Hex Gambit port `8080` first, then starts the server.

Run initial branch analysis from repo root:

`cargo run -p initial-branch-analysis -- --white12 --budget 10 --holdout-sims 0`

Default analysis outputs are written under:

- `data/analysis/`

Default input data lives under:

- `data/`
