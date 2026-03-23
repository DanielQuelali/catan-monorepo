# fastcore

Deterministic simulation core for Catan.

## What is in this crate

- Array-backed game state and rule enforcement.
- Deterministic RNG and replay-safe simulation paths.
- Action encoding/decoding primitives.
- Reversible and kernel-oriented apply paths used in hot loops.
- Value-function-driven player logic and rollout helpers.

## Main binaries

- `deterministic_regression`: deterministic replay/correctness report.
- `bench_value_state`: single-thread benchmark entrypoint used by eval harness.
- `smoke`: quick deterministic simulation smoke runner.
- `log_value_state`: action-log helper used by fixture-generation scripts.

## Typical commands

Run tests:

```bash
cargo test -p fastcore --all-features
```

Run deterministic regression binary:

```bash
cargo run -p fastcore --bin deterministic_regression -- --help
```

Run benchmark binary:

```bash
cargo run -p fastcore --release --bin bench_value_state -- --help
```
