# Single-CPU Performance Autoresearch Implementation Plan

Date: 2026-03-14

## Purpose

Define an implementation plan for running an `autoresearch`-like workflow in this repo, but aimed at single-CPU performance optimization instead of model quality optimization.

The target outcome is a system that allows many candidate optimization branches to be explored in parallel while preserving a single immutable evaluator and a reliable keep/discard decision process.

This document is intentionally implementation-oriented. It is written so a software engineer can build the missing pieces without having to infer the operating model.

## Core Goal

Increase single-CPU playout throughput as much as possible without changing results.

In this repo, that means:
- single-thread behavior only
- exact correctness preservation on a declared corpus
- benchmark decisions based on one fixed evaluator
- no candidate branch is allowed to redefine success while being judged

## Design Principle From `autoresearch`

The important pattern from `/tmp/autoresearch` is:
- immutable evaluator
- narrow mutable surface
- one scalar objective
- simple keep/discard loop
- append-only experiment history

In `autoresearch`, `prepare.py` owns the fixed metric and constants, `train.py` is the mutable surface, and the loop in `program.md` repeatedly makes a change, evaluates it, and either keeps or discards it.

The equivalent target shape in this repo is:
- immutable evaluator under `evals/single_thread/`
- mutable optimization surface in the relevant Rust implementation code
- one primary performance score: `cpu_ns_per_playout`
- one binary correctness gate before any performance decision
- one append-only experiment ledger

## Current Starting Point

The repo already has the first layer of this system:

### Existing Correctness Layer

- `evals/single_thread/generate_golden.sh`
- `evals/single_thread/verify_against_golden.sh`

This legacy layer already provides exact-output regression checking for:
- `fastcore` deterministic regression
- normalized `initial-branch-analysis` outputs

### Existing Expanded Evaluator Layer

- `evals/single_thread/correctness_manifest.json`
- `evals/single_thread/run_correctness_suite.sh`
- `evals/single_thread/benchmark_manifest.json`
- `evals/single_thread/benchmark_candidate.sh`
- `evals/single_thread/benchmark_pair.sh`
- `evals/single_thread/benchmark_compare.py`

This is enough for:
- manual correctness golden generation
- manual correctness verification of a candidate worktree
- manual paired benchmark comparisons between a baseline worktree and a candidate worktree

### What Does Not Exist Yet

The repo does not yet have:
- a campaign object
- a controller
- a benchmark queue
- benchmark-lane locking
- a canonical results ledger wired into the normal path
- baseline advancement rules
- worktree lifecycle automation
- artifact retention rules
- swarm prompts and execution policies

That is the implementation gap this document addresses.

## Non-Goals

This plan does not target:
- multithreaded optimization
- UI benchmarking
- holdout-mode tuning as a separate evaluator class
- automatic source-code generation logic for candidate ideas
- hardware-counter-driven acceptance decisions

Hardware counters may be added later as diagnostics, but they are not part of the minimum viable implementation.

## Target End State

The desired end state is a repeatable campaign workflow with the following properties:

1. A human or controller freezes a campaign baseline.
2. Correctness goldens for that campaign are generated and declared canonical.
3. Multiple candidate agents work in separate git worktrees.
4. Candidate agents may modify only the mutable performance surface.
5. Each candidate must pass correctness before it can be benchmarked.
6. Final performance measurement runs on one serialized benchmark lane only.
7. Each benchmark result produces a deterministic keep/discard decision.
8. Every candidate attempt is appended to a canonical experiment ledger.
9. When the baseline moves forward, stale candidates are invalidated or requeued under clear rules.

## Implementation Principles

The implementation should obey these principles:

### 1. Evaluator Immutability

During a campaign, the following must be treated as frozen:
- correctness manifest
- benchmark manifest
- golden artifacts
- benchmark entrypoints
- acceptance thresholds
- results schema

Changing any of those is evaluator-maintenance work, not candidate optimization work.

### 2. Narrow Mutable Surface

Candidates should be limited to performance-relevant code paths, primarily:
- `crates/fastcore/src/**`
- `crates/initial-branch-analysis/src/**`

If changes outside this area are needed, the controller should require explicit justification.

### 3. Separation of Roles

The system should distinguish:
- controller
- candidate worker
- correctness runner
- benchmark worker
- baseline manager

One process may play multiple roles in a small pilot, but the responsibilities must remain conceptually separate.

### 4. Reproducibility Over Convenience

Benchmark throughput should not be optimized at the cost of measurement validity.

That means:
- no concurrent final benchmark runs on the same machine
- no casual evaluator edits mid-campaign
- no baseline ambiguity
- no ad hoc result logging

## System Components

The implementation should introduce the following components.

## 1. Campaign Definition

Introduce a campaign concept as the top-level unit of execution.

Each campaign should define:
- campaign ID
- baseline commit
- benchmark suite
- correctness suite
- canonical golden directory
- benchmark threshold
- critical workload regression limit
- benchmark host identity or role
- canonical results ledger path
- artifact root
- campaign status

The campaign should be immutable after activation except for explicit baseline advancement events.

### Recommended Storage

Store campaign configuration in a dedicated artifact directory under a stable root such as:
- `evals/artifacts/perf_campaigns/<campaign-id>/`

Within that directory, include:
- campaign metadata
- frozen manifest copies or manifest references
- baseline metadata
- artifact subdirectories
- ledger file
- queue state

## 2. Git Backbone and Worktree Model

Git must be treated as the operational backbone of the system, not just a code hosting detail.

The implementation should define:
- one baseline branch per campaign
- one candidate branch per agent attempt
- one worktree per active candidate
- one fixed baseline worktree for benchmarking

### Branch Model

Define the following branch classes:

#### Baseline Branch

Purpose:
- represent the accepted optimization frontier for the campaign

Properties:
- exactly one baseline branch is active per campaign
- every benchmark comparison uses the current baseline commit
- only accepted candidates may advance this branch

Recommended naming:
- `perf/<campaign-id>/baseline`

#### Candidate Branch

Purpose:
- isolate one candidate optimization attempt

Properties:
- created from the current baseline commit
- belongs to one worker or one optimization topic
- never reused for unrelated attempts

Recommended naming:
- `perf/<campaign-id>/<agent-id>/<topic>`

### Worktree Model

Each active branch should have a corresponding worktree.

Recommended directories:
- baseline worktree: `/tmp/catan-perf-<campaign-id>-baseline`
- candidate worktree: `/tmp/catan-perf-<campaign-id>-<agent-id>`

### Worktree Lifecycle

The controller should own the full worktree lifecycle:

1. Create candidate branch from current baseline commit.
2. Create candidate worktree.
3. Assign worktree to worker.
4. Worker develops and commits exactly one candidate or a small bounded series if policy allows.
5. Controller records the candidate commit and submits it for evaluation.
6. After decision, controller either:
   - keeps the branch for record and archives its artifacts, or
   - deletes/prunes the worktree after recording the result.

### Worktree Invariants

The system must enforce:
- one candidate worktree belongs to one candidate branch only
- no worker edits the baseline worktree
- benchmark worker never benchmarks from a dirty worktree
- every evaluated candidate commit is recorded before cleanup

### Baseline Advancement Rules in Git Terms

When a candidate is accepted:
- baseline advancement must be explicit
- the campaign metadata must update the baseline commit
- future candidates must branch from the new baseline

Queued candidates created before advancement must be treated according to a defined policy:
- either invalidate them and require rebase/recreate
- or allow evaluation against the old baseline and mark them stale

The recommended policy is invalidation plus recreation, because it keeps comparisons consistent.

## 3. Immutable Evaluator Layer

The evaluator layer already exists, but the implementation must operationalize it as frozen campaign infrastructure.

The canonical evaluator files are:
- `evals/single_thread/correctness_manifest.json`
- `evals/single_thread/benchmark_manifest.json`
- `evals/single_thread/run_correctness_suite.sh`
- `evals/single_thread/benchmark_candidate.sh`
- `evals/single_thread/benchmark_pair.sh`
- `evals/single_thread/benchmark_compare.py`

### Required Operational Behavior

For a campaign:
- record the exact evaluator version in campaign metadata
- record the exact baseline commit and manifest paths
- reject candidate submissions that modify evaluator files
- reject candidate submissions that modify frozen manifest inputs

### Preflight Enforcement

Implement a preflight validation step before evaluation that checks:
- candidate worktree is clean except for committed changes
- candidate branch contains a valid head commit
- candidate diff does not touch forbidden evaluator files
- campaign metadata is present and valid

If any preflight check fails, mark the candidate as `build_fail` or `policy_fail` depending on the failure type.

## 4. Correctness Gate

The correctness gate must be the first mandatory evaluation phase.

### Input

The correctness gate takes:
- candidate worktree root
- campaign correctness suite
- canonical golden directory
- output artifact directory

### Behavior

It should:
- run the legacy deterministic harness if included by the correctness manifest
- run the expanded manifest-driven correctness corpus
- store raw and normalized outputs under the candidate’s artifact directory
- produce a single correctness status

### Output

Correctness status should be one of:
- `pass`
- `fail`
- `error`

`fail` means output drift.

`error` means the gate could not be completed due to build/runtime/operator issues.

### Artifact Requirements

Store:
- legacy verify outputs
- expanded correctness case outputs
- per-case status
- aggregate correctness result

### Golden Provisioning

The system must include a formal step for generating canonical goldens at campaign start.

The implementation should require:
- explicit campaign initialization
- golden generation before candidate evaluation
- storage of canonical goldens under the campaign artifact root

Do not rely on ad hoc goldens in `/tmp` for real campaigns.

## 5. Benchmark Lane

The benchmark lane is the core of the performance evaluator.

It must be treated as a single serialized resource.

### Input

The benchmark lane takes:
- baseline worktree root
- candidate worktree root
- benchmark suite
- correctness status
- campaign thresholds
- output artifact directory

### Required Behavior

It must:
- run only after correctness passes
- run one candidate at a time on the benchmark host
- run paired baseline/candidate comparisons
- alternate baseline-first and candidate-first order
- record per-repeat, per-workload outputs
- compute a deterministic summary and decision

### Benchmark Metric

Primary metric:
- `cpu_ns_per_playout`

Lower is better.

This metric must remain the primary acceptance metric for the campaign.

### Acceptance Rule

The campaign should define:
- minimum median corpus-wide speedup percentage
- maximum allowed slowdown on critical workloads

The benchmark worker should apply the rule automatically and emit:
- `keep`
- `discard`
- `benchmark_fail`

### Serialization Requirement

The implementation must ensure only one final benchmark job runs at a time on the benchmark host.

This requires one of:
- a dedicated benchmark worker process that drains a queue
- a lockfile-guarded launcher with strong ownership rules

The preferred design is a dedicated benchmark worker.

## 6. Benchmark Queue

Introduce a persistent benchmark queue between correctness and benchmark execution.

### Queue Responsibility

The queue exists to:
- decouple candidate generation from benchmark execution
- serialize the benchmark lane
- provide visibility into pending work
- support recovery after worker restarts

### Queue Item Fields

Each queued candidate should include:
- campaign ID
- candidate branch
- candidate commit
- baseline commit snapshot at enqueue time
- candidate worktree path
- status
- description
- enqueue timestamp

### Queue States

Recommended states:
- `queued`
- `running_correctness`
- `correctness_fail`
- `queued_for_benchmark`
- `running_benchmark`
- `keep`
- `discard`
- `build_fail`
- `benchmark_fail`
- `stale`
- `cancelled`

### Stale Candidate Policy

If baseline advances while a candidate is still queued or not yet benchmarked, the system should mark the candidate `stale`.

Recommended rule:
- do not benchmark stale candidates automatically
- require re-creation from the new baseline

## 7. Canonical Experiment Ledger

The system needs one canonical append-only ledger per campaign.

This ledger is the operational equivalent of `results.tsv` in `autoresearch`.

### Minimum Fields

For each candidate attempt, record:
- timestamp
- campaign ID
- candidate branch
- candidate commit
- baseline commit
- baseline branch
- correctness status
- benchmark status
- decision status
- median speedup percentage
- min speedup percentage
- max slowdown percentage
- representative `cpu_ns_per_playout`
- artifact root
- short description

### Behavior

The ledger should:
- be append-only
- record failures as well as successes
- be written by the controller or benchmark worker, not by candidate workers
- remain stable across campaign restarts

### Failure Recording

Even rejected or broken runs must be logged:
- `build_fail`
- `correctness_fail`
- `benchmark_fail`
- `discard`

This is necessary to prevent repeated wasted work and to preserve campaign history.

## 8. Artifact Layout

The implementation should define one canonical artifact layout so that every component writes outputs in a predictable place.

### Recommended Root

- `evals/artifacts/perf_campaigns/<campaign-id>/`

### Recommended Subdirectories

- `campaign/`
  - campaign metadata
  - frozen evaluator metadata
  - baseline metadata
- `goldens/`
  - canonical correctness goldens
- `queue/`
  - queue state
- `ledger/`
  - append-only experiment ledger
- `candidates/<candidate-id>/`
  - candidate metadata
  - correctness artifacts
  - benchmark artifacts
  - summary
  - logs
- `baseline/`
  - accepted baseline snapshots and advancement records

### Candidate Artifact Contents

Per candidate, retain:
- candidate metadata
- policy/preflight result
- correctness outputs
- paired benchmark repeat outputs
- `summary.json`
- human-readable decision note if one exists

## 9. Controller

The missing core component is a controller that orchestrates the full workflow.

### Controller Responsibilities

The controller should:
- initialize campaigns
- freeze the campaign baseline
- generate or validate canonical goldens
- create candidate branches and worktrees
- assign work items
- run preflight checks
- submit candidates to correctness
- enqueue benchmark jobs
- append to the ledger
- advance the baseline when a candidate is accepted
- invalidate stale candidates when needed
- clean up completed worktrees according to policy

### Controller Interface

The controller does not need to be complex initially, but it should provide explicit actions for:
- campaign initialization
- candidate creation
- candidate submission
- queue inspection
- benchmark worker execution
- baseline advancement
- worktree cleanup

### Controller Ownership

The controller, not the individual candidate worker, should own:
- branch creation
- worktree creation
- queue status
- ledger writes
- baseline advancement

This keeps the swarm from mutating campaign state inconsistently.

## 10. Candidate Workers

Candidate workers are the swarm agents that propose implementation changes.

### Candidate Worker Responsibilities

Candidate workers may:
- inspect profiles
- edit mutable performance code
- build locally
- run lightweight smoke checks
- commit candidate changes

Candidate workers may not:
- edit evaluator files
- advance the baseline
- write to the canonical ledger
- bypass the queue
- run production benchmark decisions outside the benchmark lane

### Candidate Worker Output

Each candidate worker should produce:
- one branch
- one candidate commit
- one short description
- one submission event to the controller

## 11. Benchmark Worker

Introduce a dedicated benchmark worker role.

### Benchmark Worker Responsibilities

The benchmark worker should:
- drain the queue one candidate at a time
- run correctness if the queue design requires it, or consume correctness-pass candidates only
- execute the paired benchmark protocol
- write summary artifacts
- write the decision row to the ledger
- notify controller logic of accepted candidates

### Benchmark Worker Restrictions

The benchmark worker should be the only component allowed to run production `benchmark_pair.sh` decisions for an active campaign.

This is the simplest way to enforce benchmark serialization.

## 12. Baseline Management

The system must define exactly how the accepted frontier moves.

### Baseline Advancement Event

When a candidate is accepted:
- record the acceptance in campaign metadata
- record the new baseline commit
- update the baseline branch pointer
- create a baseline advancement record in artifacts

### Candidate Compatibility Rule

Once the baseline advances:
- unevaluated candidates based on the old baseline should be marked stale
- fresh candidates must be branched from the new baseline

### Why This Matters

Without this rule, the campaign will compare candidates against different baselines and the ledger will stop being interpretable.

## 13. Benchmark Host Discipline

The software implementation is only part of the system. The benchmark environment also needs explicit rules.

### Host Requirements

The benchmark host should have:
- stable power mode
- no unrelated heavy workloads
- one reserved benchmark lane
- predictable filesystem and cache behavior

### Noise Characterization

Before launching a real campaign:
- run baseline-vs-baseline paired comparisons repeatedly
- measure per-workload noise
- confirm that the default acceptance threshold is above measurement noise

### Threshold Confirmation

The current threshold is a reasonable starting point, not a proven constant.

The implementation should make threshold values explicit in campaign configuration and require campaign initialization to choose them intentionally.

## 14. Prompt and Policy Packaging

The repo also needs reproducible operating instructions for the swarm.

### Required Prompt Classes

Define and store:
- candidate-worker prompt
- controller prompt
- benchmark-worker prompt

These prompts should explicitly state:
- immutable evaluator files
- mutable performance surface
- queue discipline
- baseline advancement rules
- artifact and ledger expectations

This should live in-repo so a future operator does not have to reconstruct process from memory.

## 15. Recommended Repository Additions

The following additions are recommended to implement this plan.

### New Operational Documents

Add:
- campaign runbook
- benchmark host runbook
- swarm role prompts
- baseline advancement policy

### New Operational Scripts or Modules

Add components for:
- campaign initialization
- worktree creation and cleanup
- candidate submission
- queue management
- benchmark worker execution
- campaign inspection/status reporting

The exact language is not important. The behavior and ownership boundaries are.

## 16. Phased Implementation Plan

Implement in phases.

## Phase 1: Stabilize Campaign Inputs

Objective:
- make one campaign reproducible by freezing baseline, goldens, manifests, and thresholds

Tasks:
- define campaign metadata schema
- define campaign artifact root
- define canonical golden location
- implement campaign initialization flow
- record baseline commit explicitly
- copy or reference frozen manifest inputs

Acceptance criteria:
- a campaign can be initialized once
- canonical goldens can be generated once
- the campaign state is inspectable without human memory

## Phase 2: Add Git Backbone and Worktree Automation

Objective:
- make candidate branch and worktree creation deterministic and controller-owned

Tasks:
- define branch naming scheme
- define worktree naming scheme
- implement create/list/remove operations
- persist candidate metadata
- ensure candidate branches start from current baseline commit

Acceptance criteria:
- controller can create and clean candidate worktrees
- no ambiguity exists about which worktree belongs to which candidate
- candidate metadata records the baseline commit it was created from

## Phase 3: Add Queue and Ledger

Objective:
- move from ad hoc evaluation to a durable campaign workflow

Tasks:
- implement persistent queue
- implement canonical append-only ledger
- define candidate states
- wire correctness and benchmark outputs into the ledger

Acceptance criteria:
- pending, running, failed, accepted, and stale candidates are all visible
- campaign history survives restarts
- no evaluation result is lost

## Phase 4: Add Serialized Benchmark Worker

Objective:
- enforce correct final benchmarking behavior

Tasks:
- implement benchmark worker ownership
- enforce one-at-a-time benchmark execution
- wire queue dequeue into benchmark execution
- write benchmark summaries and decisions automatically

Acceptance criteria:
- benchmark worker is the only production path for final benchmark decisions
- concurrent benchmark execution is prevented
- keep/discard decisions are automatic and reproducible

## Phase 5: Add Baseline Advancement Logic

Objective:
- make accepted candidates advance the campaign frontier correctly

Tasks:
- implement explicit baseline advancement event
- update campaign metadata
- mark older queued candidates stale
- define requeue or recreate policy

Acceptance criteria:
- every accepted candidate has a recorded advancement event
- no queued candidate is evaluated against an ambiguous baseline

## Phase 6: Add Swarm Packaging

Objective:
- make the workflow operable by a real swarm instead of one careful human

Tasks:
- write standard prompts
- package controller and worker usage docs
- define candidate submission protocol
- define cleanup policy

Acceptance criteria:
- a new operator can launch a small campaign from the repo docs alone
- worker behavior is consistent enough to avoid evaluator drift

## 17. Recommended First Milestone

The first milestone should not attempt the full overnight swarm.

The first milestone should deliver a small but real pilot:
- one campaign
- one frozen baseline
- one canonical `gate` correctness corpus
- one baseline worktree
- two or three candidate worktrees
- one persistent queue
- one serialized benchmark worker
- one canonical results ledger

If that pilot works reliably, the system is ready for broader swarm scaling.

## 18. Definition of Done

This implementation should be considered complete for the first production-ready version when all of the following are true:

1. A campaign can be initialized from a baseline commit with canonical goldens.
2. Candidate branches and worktrees are created automatically from the active baseline.
3. Candidate workers cannot modify frozen evaluator files without being rejected.
4. Every candidate enters a durable queue and gets a durable ledger row.
5. Correctness runs before benchmarking and blocks invalid candidates.
6. Final benchmarking is serialized and reproducible.
7. Accepted candidates advance the baseline under explicit recorded rules.
8. Stale candidates are handled deterministically after baseline changes.
9. Artifact layout is stable and sufficient for postmortem analysis.
10. A software engineer can operate one campaign end to end without relying on undocumented tribal knowledge.

## Bottom Line

The repo already has the evaluator core needed for an `autoresearch`-like performance workflow.

What is missing is the operational system around it:
- campaign definition
- git/worktree backbone
- queue
- benchmark worker
- ledger
- baseline advancement
- swarm packaging

That is the implementation target. Once those pieces exist, the repo will be in a practical state for autonomous single-CPU performance optimization campaigns.
# Superseded

This document reflects an older evaluator design that included `initial-branch-analysis`.
The active harness under `evals/single_thread/` is now playout-only (`fastcore` correctness plus `bench_value_state` benchmark).
