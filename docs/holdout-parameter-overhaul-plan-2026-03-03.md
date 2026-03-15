# Holdout Parameter Overhaul Plan (2026-03-03)

Status: Planned  
Owner: Engineering  
Source incident: `docs/holdout-replay-incident-postmortem-2026-03-03.md`

## 1. Objective

Refactor all parameter surfaces that can cause replay/rerun/reuse confusion, then retire ambiguous names through compatibility aliases and explicit validation.

## 2. Scope

In scope:
- `crates/initial-branch-analysis` CLI parsing and validation
- `scripts/run_opening_white12_analysis.py` wrapper CLI
- Operator documentation and templates

Out of scope:
- Simulation algorithm changes
- CSV schema changes unrelated to holdout mode semantics

## 3. Parameter Inventory and Canonicalization

| Surface | Current parameter | Confusion risk | Canonical target | Compatibility policy |
|---|---|---|---|---|
| Rust CLI | `--holdout-rerun` | Mistaken for replay | `--holdout-mode rerun` | Keep alias with warning |
| Rust CLI | `--holdout-replay <path>` | Used without explicit mode | `--holdout-mode replay` + replay path | Keep path arg, require mode match |
| Rust CLI | implicit reuse (absence of rerun/replay) | Hidden default mode | `--holdout-mode reuse` | Default remains `reuse`, print resolved mode |
| Rust CLI | `--holdout-only` | Sounds like mode, not output scope | `--all-sims-scope holdout` | Keep alias with warning |
| Rust CLI | `--holdout-sims` | Duplicate of `--num-sims` | `--num-sims` | Keep alias with warning |
| Rust CLI | silent numeric fallback defaults | Bad values can be ignored | strict parse errors | Remove silent fallback for critical flags |
| Rust CLI | inferred branch mode from state `current_color` | Implicit behavior | explicit startup echo with inferred value | Keep behavior, document clearly |
| Python wrapper | `--holdout-rerun` only | Missing explicit mode model | `--holdout-mode replay|rerun|reuse` | Keep alias with warning |
| Python wrapper | `--holdout-only` only | Output scope ambiguity | `--all-sims-scope all|holdout` | Keep alias with warning |
| Python wrapper | no direct replay-mode surface | Replay requests easy to mis-execute | expose `--holdout-replay` + mode validation | New canonical params |

## 4. Validation Matrix (Required)

### Holdout mode rules

1. `replay`:
- requires `--holdout-replay <path>`
- forbids rerun-only semantics

2. `rerun`:
- requires `--num-sims > 0`
- forbids `--holdout-replay`

3. `reuse`:
- forbids `--holdout-replay`

### Output scope rules

1. `--all-sims-scope all`:
- emit TS + holdout all-sims outputs

2. `--all-sims-scope holdout`:
- emit holdout-only all-sims outputs

### Parse behavior rules

1. Invalid combinations fail before simulation work.
2. Missing required value for a flag fails at parse time.
3. Deprecated aliases emit warnings and normalize to canonical values.

## 5. Resolved-Config Echo (Required)

Before long-running work begins, print a single resolved line containing:
- holdout mode
- output scope
- board/sample scope
- sims
- budget
- replay path (or empty)
- output paths

## 6. Docs Rollout Plan

1. Publish runbook:
- `docs/holdout-modes-runbook.md`

2. Update artifact spec examples to canonical mode language:
- `docs/hex-gambit-analysis-artifact-spec.md`

3. Keep postmortem action list synchronized with implementation status.

## 7. Implementation Order

1. Rust CLI parser canonicalization + validation + deprecation warnings.
2. Rust parse-matrix unit tests.
3. Python wrapper canonicalization + validation + deprecation warnings.
4. Replace doc examples to canonical commands.
5. Remove ambiguous examples and anti-patterns from all operator docs.

## 8. Exit Criteria

1. Replay requests cannot run unless mode resolves to `replay`.
2. Rerun cannot run with `num-sims <= 0`.
3. Reuse/rerun cannot accept replay path.
4. Operators can identify mode and scope from one resolved startup line.
5. No public docs present ambiguous replay/rerun commands as equivalent.

