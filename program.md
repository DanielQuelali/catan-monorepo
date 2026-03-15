# single-cpu playout autoresearch

This is an experiment to have the LLM do its own performance research.

## Setup

To set up a new experiment, work with the user to:

1. **Agree on a campaign id**: propose a tag based on today's date (e.g. `mar15`). The branch `autoresearch/<campaign-id>` must not already exist. This is a fresh run.
2. **Create the coordinator branch**: `git checkout -b autoresearch/<campaign-id>` from the current branch.
3. **Initialize the campaign**: create one campaign from the current baseline. This freezes the benchmark/correctness manifests, creates the baseline branch/worktree, and installs `campaign/results.tsv` plus `campaign/analysis.ipynb`.

```bash
evals/single_thread/perf_campaign.sh init \
  --campaign-id <campaign-id> \
  --baseline-ref HEAD \
  --benchmark-suite gate \
  --correctness-suite gate \
  --generate-goldens
```

4. **Read the in-scope files**: Read these files for full context:
   - `README.md` — repository context.
   - `evals/single_thread/README.md` — evaluator context.
   - `evals/single_thread/perf_campaign.py` — campaign controller.
   - `evals/single_thread/benchmark_manifest.json` — fixed benchmark corpus.
   - `evals/single_thread/correctness_manifest.json` — fixed correctness corpus.
5. **Verify goldens exist**: Check that the chosen golden dir contains the required `engine_report.json` files. If not, tell the human to initialize with `--generate-goldens` or generate them before starting.
6. **Verify the git backbone exists**: inspect the coordinator branch, baseline branch, and worktrees.

```bash
git status --short --branch
git rev-parse HEAD
git worktree list
evals/single_thread/perf_campaign.sh status --campaign-id <campaign-id>
```

7. **Confirm and go**: Confirm setup looks good.

Once you get confirmation, kick off the experimentation.

## Experimentation

Each experiment runs on a single CPU benchmark lane. The evaluator is fixed and the optimizer changes the implementation, not the judge.

**What you CAN do:**
- Modify `crates/fastcore/src/**`. This is the code you optimize.

**What you CANNOT do:**
- Modify the evaluation harness. These files are read-only:
  - `evals/single_thread/generate_golden.sh`
  - `evals/single_thread/verify_against_golden.sh`
  - `evals/single_thread/correctness_manifest.json`
  - `evals/single_thread/run_correctness_suite.sh`
  - `evals/single_thread/benchmark_manifest.json`
  - `evals/single_thread/benchmark_candidate.sh`
  - `evals/single_thread/benchmark_pair.sh`
  - `evals/single_thread/benchmark_compare.py`
- Install new packages or add dependencies.
- Modify campaign metadata, queue, results, or ledger files directly.

**The goal is simple: increase playouts per CPU second without changing results.** The primary metric is `playouts_per_cpu_second`. The equivalent lower-is-better metric is `cpu_ns_per_playout`. Since the evaluator is fixed, you do not need to worry about changing the benchmark methodology. Everything is fair game inside `crates/fastcore/src/**` as long as the code remains deterministic and passes correctness.

**Correctness** is a hard constraint. A candidate must pass deterministic playout regression before it is benchmarked.

**Simplicity criterion**: All else being equal, simpler is better. A small improvement that adds ugly complexity is not worth it. Conversely, removing something and getting equal or better results is a great outcome. When evaluating whether to keep a change, weigh the complexity cost against the improvement magnitude. A tiny performance gain that adds 20 lines of hacky code? Probably not worth it. A similar gain from deleting code? Definitely keep. An improvement of ~0 but much simpler code? Probably still discard under the fixed 5% rule, but note it as an interesting simplification.

**The first run**: Your very first run should always be to establish the baseline campaign state and verify the worker/evaluator path before speculative edits.

## Output format

Once the benchmark pair finishes it writes a `summary.json` like this:

```json
{
  "median_speedup_pct": 5.4321,
  "playouts_per_cpu_second": 512.34,
  "cpu_ns_per_playout": 1953125.0,
  "baseline_total_playouts_per_repeat": 8192,
  "candidate_total_playouts_per_repeat": 8192,
  "total_paired_playouts_all_repeats": 81920,
  "status": "keep"
}
```

You can extract the key metric quickly from the summary:

```bash
python3 - <<'PY'
import json
print(json.load(open("summary.json"))["median_speedup_pct"])
PY
```

## Logging results

When an experiment is done, it is logged automatically to `campaign/results.tsv` (tab-separated, NOT comma-separated).

The TSV has a header row and 5 columns:

```text
commit	median_speedup_pct	playouts_per_cpu_second	status	description
```

1. git commit hash (short, 7 chars)
2. median speedup achieved (e.g. `5.432100`) — use `0.000000` when no benchmark result exists
3. playout throughput (e.g. `512.34`) — use `0.00` when no benchmark result exists
4. status: usually `keep` or `discard`, but terminal failure states may also appear
5. short text description of what this experiment tried

Example:

```text
commit	median_speedup_pct	playouts_per_cpu_second	status	description
a1b2c3d	0.000000	0.00	keep	baseline
b2c3d4e	5.432100	512.34	keep	cache winner in hot loop
c3d4e5f	1.203400	498.90	discard	inline helper only
d4e5f6g	0.000000	0.00	correctness_fail	change altered per-seed trace hash
```

The campaign also maintains:

- `campaign/analysis.ipynb`
- `evals/artifacts/perf_campaigns/<campaign-id>/ledger/experiments.jsonl`

Use `results.tsv` and `analysis.ipynb` for quick human inspection and the ledger for full fidelity. Do not commit the campaign artifacts.

## The experiment loop

The experiment runs on a dedicated coordinator branch (for example `autoresearch/mar15`) plus disposable candidate branches/worktrees rooted at the current accepted baseline.

LOOP FOREVER:

1. Look at the git state: the current coordinator branch/commit, the baseline branch/worktree, and recent results.

```bash
git status --short --branch
git rev-parse HEAD
git worktree list
evals/single_thread/perf_campaign.sh status --campaign-id <campaign-id>
```

2. Create one candidate branch and worktree:

```bash
evals/single_thread/perf_campaign.sh create-candidate \
  --campaign-id <campaign-id> \
  --agent-id <agent-id> \
  --topic <short-topic>
```

3. Enter the candidate worktree and confirm the git state:

```bash
cd <candidate-worktree>
git status --short --branch
git rev-parse --abbrev-ref HEAD
git rev-parse HEAD
```

4. Tune `crates/fastcore/src/**` with an experimental idea by directly hacking the code.
5. git commit

```bash
git status --short
git add crates/fastcore/src
git commit -m "<short experiment description>"
```

6. Submit the candidate:

```bash
evals/single_thread/perf_campaign.sh submit-candidate \
  --campaign-id <campaign-id> \
  --candidate-id <candidate-id>
```

7. Run the experiment worker:

```bash
evals/single_thread/perf_campaign.sh worker-run \
  --campaign-id <campaign-id> \
  --drain \
  --continuous \
  --poll-seconds 15
```

8. Read out the results from `campaign/results.tsv`, `campaign/analysis.ipynb`, and the candidate `summary.json`.
9. If the candidate improved enough, you "advance" the baseline, keeping the git commit:

```bash
evals/single_thread/perf_campaign.sh advance-baseline \
  --campaign-id <campaign-id> \
  --candidate-id <candidate-id>
```

10. If the candidate is equal or worse, or fails correctness/benchmarking, discard it and move on. Do not edit the same failed candidate branch forever. Make a new candidate and try again.

The idea is that you are a completely autonomous researcher trying things out. If they work, keep. If they do not, discard. And you are advancing the accepted baseline so that you can iterate.

## Correctness

Correctness is playout-only.

It uses `fastcore --bin deterministic_regression` and compares:
- aggregate stats
- per-seed winner
- per-seed turn count
- per-seed illegal-action count
- per-seed full-log `blake3` trace hash

On correctness failure, inspect:
- `failure_dump/engine_report.diff`
- `failure_dump/mismatch_summary.json`
- `failure_dump/candidate_logs/seed_<N>.log`

Do not benchmark a candidate that fails correctness.

## Benchmark discipline

Do not run multiple final benchmark jobs at the same time on the same benchmark machine.

Parallelize:
- idea generation
- code editing
- local inspection
- non-final smoke checks

Serialize:
- final `benchmark_pair.sh` runs on the benchmark lane

Reason:
- shared cache, memory bandwidth, scheduler drift, and thermal drift will corrupt single-CPU timing

## Timeout

Each experiment has hard phase timeouts enforced by the campaign worker. If correctness or benchmarking exceeds the frozen campaign timeout, it is killed and treated as a failure.

The defaults are:
- correctness timeout: 1800 seconds
- benchmark timeout: 3600 seconds

## Crashes

If a candidate crashes, times out, fails correctness, or fails benchmarking, use your judgment: If it is something dumb and easy to fix, fix it in a new candidate and re-run. If the idea itself is fundamentally broken, just skip it, let the failure be logged, and move on.

## NEVER STOP

Once the experiment loop has begun, do NOT pause to ask the human if you should continue. Do NOT ask whether you should keep going. The human might be asleep, or away from a computer, and expects you to continue working indefinitely until manually stopped.

If you run out of ideas, think harder:
- re-read `crates/fastcore/src/**`
- inspect hot paths
- look for unnecessary allocation, copying, branching, or recomputation
- combine previous near-misses
- try more radical but still deterministic ideas

The loop runs until the human interrupts you, period.
