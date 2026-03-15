# Holdout Replay Incident Post-Mortem (2026-03-03)

Status: Open (docs + plan expanded, implementation pending)  
Date of incident: March 3, 2026  
Scope: `data/analysis/opening_states/0003..0009` holdout artifact generation

## 1. Executive Summary

A request to generate holdout CSVs "through replay" for boards `0003` to `0009` in `tmux` was executed with the wrong mode:

- Requested intent: replay mode (`--holdout-replay <path>`)
- Executed mode: rerun mode (`--holdout-rerun`)

This triggered the expensive TS exploration + holdout rerun path instead of replay-row evaluation. The wrong process was later stopped.

## 2. Impact

Known impact:

- Compute/time waste from running the wrong analysis path.
- Possible overwrite of derived holdout artifacts for boards that completed before stop.

Not impacted:

- No source code edits were required for the run itself.
- No destructive git/reset/delete operations were run.
- No board/state input fixtures were intentionally modified by the command.

Risk statement:

- Since analysis outputs are not versioned, any overwritten per-board holdout CSV artifacts are expensive to recover without backups.

## 3. Timeline (March 3, 2026)

1. Request received: run holdout generation "through replay" for `0003..0009` in `tmux`.
2. Incorrect command launched with `--holdout-rerun --holdout-only` (not `--holdout-replay`).
3. Run progressed through multiple boards.
4. Mismatch discovered and acknowledged.
5. Incorrect process stopped on user request.

## 4. Root Cause

Primary root cause:

- Operator interpreted "replay" as "holdout rerun" due to documentation emphasis on rerun examples.

Direct contributing causes:

- Missing explicit runbook that distinguishes replay mode vs rerun mode.
- No mandatory preflight confirmation step mapping user language to exact CLI flags.
- Existing docs include rerun examples but do not enforce mode-selection safeguards.

## 5. Parameter Semantics (Authoritative Clarification)

`--holdout-replay <path>`
- Replays explicit holdout rows from a replay CSV path.
- Intended when user asks for "replay".

`--holdout-rerun`
- Recomputes holdout summaries instead of TS-reuse summaries.
- Not equivalent to replay-file mode.

`--holdout-only`
- Output/materialization control (holdout output path behavior).
- Does not imply replay mode.

`--budget <N>`
- Controls TS exploration budget.
- High values can be very expensive.

`--num-sims <N>`
- Number of holdout simulation seeds per evaluation.

`--exclude-sample-ids ...`
- Board/sample scope filter.

## 6. Prevention Plan: Parameter Overhaul + Docs

### A. Parameter Overhaul (Required)

1. Replace ambiguous flags with a single explicit mode parameter:
   - `--holdout-mode replay|rerun|reuse`
2. Replace ambiguous output-scope flag:
   - canonical: `--all-sims-scope all|holdout`
   - compatibility alias: `--holdout-only` -> `holdout`
3. Collapse duplicate simulation-count names:
   - canonical: `--num-sims`
   - compatibility alias: `--holdout-sims`
4. Enforce mode-specific required args:
   - `replay` requires `--holdout-replay <path>`
   - `rerun` requires `--num-sims > 0`
   - `reuse` forbids `--holdout-replay`
5. Enforce mutually exclusive semantics at parse time:
   - fail fast with non-zero exit on invalid combinations
6. Add startup echo of normalized resolved config:
   - mode, board scope, sims, budget, replay path
7. Deprecate direct use of `--holdout-rerun` and keep it as a compatibility alias only:
   - alias must map internally to `--holdout-mode rerun`
   - parser must print a deprecation warning
8. Require value-parse strictness for critical flags:
   - no silent fallback defaults on malformed values
9. Mirror the same canonical parameter model in both surfaces:
   - `crates/initial-branch-analysis` binary
   - `scripts/run_opening_white12_analysis.py` wrapper

### B. Documentation Overhaul (Required)

1. Create a dedicated runbook: "Holdout Modes and Commands".
2. Add a command truth table:
   - user intent -> required mode -> required flags -> forbidden flags
3. Add copy-paste command templates for each mode:
   - replay, rerun, reuse
4. Add an anti-pattern section with explicit examples of wrong combinations.
5. Add a mandatory preflight checklist in docs:
   - intended mode
   - exact command
   - board scope
   - output path

### C. Execution Policy

1. Any request containing "replay" must resolve to replay mode in the final command.
2. If replay path is missing, execution is blocked until path is explicitly provided.
3. No long-running launch without printing the resolved mode line first.

## 7. Action Items

1. Implement `--holdout-mode replay|rerun|reuse` in `initial-branch-analysis`.
2. Add parse-time validation matrix tests for all mode/flag combinations.
3. Implement `--all-sims-scope all|holdout` and keep `--holdout-only` as deprecation alias.
4. Deprecate `--holdout-sims` in favor of `--num-sims`.
5. Update `scripts/run_opening_white12_analysis.py` to expose canonical params:
   - `--holdout-mode`, `--all-sims-scope`, `--holdout-replay`.
6. Publish `docs/holdout-modes-runbook.md` with templates and anti-patterns.
7. Publish `docs/holdout-parameter-overhaul-plan-2026-03-03.md` with rollout phases and owner checklist.
8. Update `docs/hex-gambit-analysis-artifact-spec.md` to reference the runbook and mode rules.

## 8. Definition of Done

1. Replay requests cannot execute unless mode resolves to `replay`.
2. Invalid mode/flag combinations fail before any simulation work starts.
3. All operator docs use canonical interfaces:
   - `--holdout-mode`
   - `--all-sims-scope`
   - `--num-sims`
4. No documentation examples remain that can be interpreted ambiguously between replay and rerun.
