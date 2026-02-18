# Fastcore Smoke Runner

This is a tiny CLI harness for running deterministic simulation batches during
development. It is designed to be fast to invoke and stable across runs.

## Usage

From `fastcore/`:

```sh
cargo run --bin smoke -- --seeds 1,2,3
```

Optional max turns override:

```sh
cargo run --bin smoke -- --seeds 1,2,3 --max-turns 200
```

If `--seeds` is omitted, the default set is `1..=10`.
