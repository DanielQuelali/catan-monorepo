# Initial Branch Analysis Simulation Kernel Plan (Local Git, LLM Agent Execution)

## Goal

Implement the first 4 throughput improvements in sequence:

1. Streaming aggregation (no per-sim `Vec<PlayoutResult>` materialization)
2. Forward-only simulation apply path (no `Delta` construction in hot playout path)
3. Clone-free decision scoring in `FastValueFunctionPlayer::decide`
4. Kernel-oriented state layout for hot fields (`road_components`, `dev_deck`)

This plan assumes local git workflow only (no GitHub PR flow).

## Agentic Best Practices (OpenAI Harness-Oriented)

- All implementers are LLM agents; every task must have:
  - one implementation agent
  - one verification agent
- Each task must define explicit input/output contract:
  - input: branch, files, exact command matrix
  - output: changed files, test logs, benchmark log, diff summary
- Keep changes small and reviewable:
  - one concern per commit
  - no mixed refactor + behavior change unless required
- Determinism first:
  - fixed seeds for all behavioral checks
  - exact output diffs (excluding approved timing fields only)
- Verification agent must re-run all required commands independently.
- If checks fail, agent must provide:
  - minimal repro command
  - suspected root-cause file/function
  - rollback/containment step

## Execution Model (Codex Harness First)

- Orchestration uses Codex agent runs, not manual git command choreography.
- Every work item is executed as:
  - one implementation run (`codex exec --json ...`)
  - one verification run (`codex exec --json ...`) with a stricter read-only/review prompt
- Use Codex session continuation for iterative loops:
  - `codex exec resume --last "<follow-up task>"`
- Use structured outputs for scoring/signoff artifacts:
  - `--output-schema <schema.json>`
  - `-o <artifact.json>`

### Command templates

Implementation run:

```bash
codex exec --json --full-auto \
  --sandbox workspace-write \
  -C /path/to/catan-monorepo \
  "Implement PR<N> from docs/simulation-kernel-implementation-plan.md.
   Respect AGENTS.md and preserve output compatibility.
   Run required tests and report exact command outputs." \
  > evals/artifacts/pr<N>.impl.trace.jsonl
```

Verification run:

```bash
codex exec --json \
  --sandbox read-only \
  -C /path/to/catan-monorepo \
  "Verify PR<N> implementation with deterministic checks.
   Execute the required test/perf matrix and emit pass/fail with evidence." \
  --output-schema evals/schemas/pr_verification.schema.json \
  -o evals/artifacts/pr<N>.verify.report.json \
  > evals/artifacts/pr<N>.verify.trace.jsonl
```

Optional follow-up:

```bash
codex exec resume --last "Address the verifier findings only; keep scope to PR<N>."
```

## Branch + Integration Model (Local Git)

- Integration branch: `perf/sim-kernel-integration`
- Feature branches:
  - `perf/pr1-streaming-aggregation`
  - `perf/pr2-forward-apply`
  - `perf/pr3-clone-free-decide`
  - `perf/pr4-state-layout-kernel`
- Merge order is strict: `pr1 -> pr2 -> pr3 -> pr4`
- Merge method: local integration after verifier artifacts pass for each PR.
- Command details for integration are intentionally omitted here; this document focuses on Codex harness execution commands.

## Test Gates (Required for Every PR)

- `cargo test -p fastcore --all-features`
- `cargo test -p initial-branch-analysis --all-features`
- Deterministic fixed-seed diff checks:
  - `fastcore` benchmark output consistency (wins/turns, excluding time fields)
  - `initial-branch-analysis` CSV consistency (excluding wall/cpu timing fields)
- Harness trace checks (from `--json` output):
  - command execution events present and ordered
  - no unexpected permission/sandbox escalation events
  - final turn completion event present
- Release perf check:
  - `cargo run --release -p fastcore --bin bench_value_state -- --state data/state_pre_last_settlement.json --board data/board_example.json --sims 200`

## Evals Harness Pattern (From Codex Blog Guidance)

- Use a small prompt suite first (10-20 prompts) and grow from failures.
- Run each prompt with `codex exec --json` and save JSONL traces.
- Score deterministic checks directly from `item.*` command events.
- Add a second rubric pass using `--output-schema` for qualitative checks.

Command skeleton:

```bash
codex exec --json --full-auto "<prompt>" > evals/artifacts/<case>.jsonl

codex exec \
  "Run read-only rubric evaluation for <case>" \
  --output-schema evals/style-rubric.schema.json \
  -o evals/artifacts/<case>.rubric.json
```

## PR1: Streaming Aggregation

### Scope

- Refactor `run_playouts` and `summarize_playouts` in `crates/initial-branch-analysis/src/main.rs`
- Replace `Vec<PlayoutResult>` return with aggregate accumulator(s)
- Keep output semantics identical

### TODO (Agent A - Implementation Owner)

- [x] Introduce accumulator struct(s) for wins, vps totals, turns, and sample counts
- [x] Refactor sequential worker path to update accumulator in-stream
- [x] Refactor parallel path to reduce per-thread accumulators
- [x] Remove no-longer-needed reducers that depend on `Vec<PlayoutResult>`
- [x] Keep `PlayoutSummary` fields/rounding behavior unchanged

### TODO (Agent B - Verification Owner)

- [x] Add/extend tests that compare old/new aggregate math on fixed synthetic results
- [x] Run deterministic seed diff script for baseline scenario + `--blue2` + `--orange2` + `--white12`
- [x] Confirm CSV schema and row ordering unchanged

## PR2: Forward-Only Apply Path (Simulation Only)

### Scope

- Add simulation-kernel apply path in `fastcore` engine
- Avoid `Delta::default()` creation in forward playout loop
- Keep reversible/undo-compatible path available for non-kernel usage

### TODO (Agent C - Implementation Owner)

- [x] Add `apply_*_kernel` path(s) for forward-only simulation mutation
- [x] Wire `simulate_from_state_with_scratch`/playout path to kernel apply APIs
- [x] Preserve existing behavior for prompts/actions and state transitions
- [x] Keep old apply APIs intact where reversibility is still needed

### TODO (Agent D - Verification Owner)

- [x] Add old-vs-new differential tests for each action prompt class
- [x] Add trajectory equivalence tests over fixed seed batches
- [x] Validate winner/turn totals remain exact matches

## PR3: Clone-Free `decide` Path

### Scope

- Refactor `FastValueFunctionPlayer::decide` in `crates/fastcore/src/value_player.rs`
- Eliminate per-candidate `state.clone()` during action scoring
- Use reversible mutation + rollback mechanism for speculative evaluation

### TODO (Agent E - Implementation Owner)

- [x] Introduce reusable scratch/reversible evaluation context for candidate scoring
- [x] Replace clone-per-action branch with apply/rollback flow
- [x] Keep action tie-break and deterministic ordering unchanged
- [x] Keep epsilon random branch behavior unchanged

### TODO (Agent F - Verification Owner)

- [x] Add tests asserting selected action equivalence on fixed states/seeds
- [x] Add full playout equivalence tests for fixed seed sets
- [x] Measure release performance delta and record before/after numbers

## PR4: Kernel State Layout

### Scope

- Rework hot mutable state fields to reduce dynamic allocation/copy costs
- Target fields currently using dynamic vectors in hot paths:
  - `road_components`
  - `dev_deck`

### TODO (Agent G - Implementation Owner)

- [x] Design fixed-size or pooled representation for road connectivity
- [x] Design kernel-friendly development deck representation
- [x] Update engine/value logic to use new representations
- [x] Preserve external behavior and serialization assumptions

### TODO (Agent H - Verification Owner)

- [x] Build migration helpers/adapters where old representation is referenced
- [x] Extend property tests for invariants after long random action sequences
- [x] Run full feature-matrix test suite and deterministic diff suite
- [x] Rebaseline and document performance impact

## Coordination Checklist (All Agents)

- [ ] Rebase feature branch on latest `perf/sim-kernel-integration` before final test run
- [ ] Do not change output schema/column names unless explicitly planned
- [ ] Keep each commit scoped to one concern; avoid mixed mechanical + logic commits
- [ ] Record benchmark command + exact output snippet in commit message trailer
- [ ] Tag any intentionally changed deterministic outputs with rationale in commit message
- [ ] Implementation agent includes a short handoff note: assumptions, touched invariants, known limits
- [ ] Verification agent signs off only with command outputs and diff evidence

## Definition of Done

- All 4 branches merged locally into `perf/sim-kernel-integration`
- Deterministic diff checks pass for target scenarios
- Full test gates pass under `--all-features`
- Release benchmark shows clear throughput improvement vs pre-PR1 baseline
