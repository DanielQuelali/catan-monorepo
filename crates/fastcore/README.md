# fastcore

Deterministic, allocation-efficient simulation core for Catanatron.

This crate is the starting point for the Rust-only simulation loop described in
`CATANATRON_OVERHAUL.md`. It currently contains scaffolding for array-backed
state, action encoding, deltas, and deterministic RNG streams. The board tables
and full rules engine will be filled in as the next milestones.
