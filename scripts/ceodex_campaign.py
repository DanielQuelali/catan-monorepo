#!/usr/bin/env python3
import argparse
import json
import os
import select
import signal
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path


TERMINAL_CANDIDATE_STATUSES = {
    "keep",
    "discard",
    "build_fail",
    "correctness_fail",
    "benchmark_fail",
    "policy_fail",
    "stale",
    "cancelled",
    "accepted",
}

TOPICS = [
    "rollout-loop",
    "allocation-churn",
    "winner-aggregation",
    "action-encoding",
    "legal-action-hotpath",
    "resource-hotpath",
    "state-mutation",
    "loop-simplification",
    "branch-elimination",
    "copy-reduction",
]

RATE_LIMIT_NEEDLE_STRINGS = (
    "usage limit",
    "try again at",
    "rate limit",
    "limit reached",
)
STALE_REPLAY_CLAIM_TTL_SECONDS = 2 * 60 * 60


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="CEOdex campaign supervisor")
    parser.add_argument("--campaign-id", required=True)
    parser.add_argument(
        "--repo-root",
        default=str(Path(__file__).resolve().parents[1]),
    )
    parser.add_argument(
        "--artifacts-root",
        default=None,
        help="Defaults to <repo-root>/evals/artifacts/perf_campaigns",
    )
    parser.add_argument("--producer-count", type=int, default=2)
    parser.add_argument("--queue-target", type=int, default=2)
    parser.add_argument("--worker-poll-seconds", type=float, default=15.0)
    parser.add_argument("--producer-poll-seconds", type=float, default=20.0)
    parser.add_argument("--cleanup-interval-seconds", type=float, default=120.0)
    parser.add_argument("--rate-limit-poll-seconds", type=float, default=300.0)
    parser.add_argument("--restart-delay-seconds", type=int, default=300)
    parser.add_argument("--heartbeat-seconds", type=float, default=30.0)
    parser.add_argument("--codex-model", default="")
    parser.add_argument("--codex-timeout-seconds", type=float, default=1800.0)
    parser.add_argument("--codex-color", default="never", choices=["always", "never", "auto"])
    return parser.parse_args()


def run(
    cmd: list[str],
    *,
    cwd: Path,
    capture: bool = False,
    input_text: str | None = None,
) -> subprocess.CompletedProcess:
    return subprocess.run(
        cmd,
        cwd=str(cwd),
        input=input_text,
        text=True,
        capture_output=capture,
        check=False,
    )


def load_json(path: Path) -> dict:
    return json.loads(path.read_text())


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def append_jsonl(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a") as handle:
        handle.write(json.dumps(payload, sort_keys=True))
        handle.write("\n")


def format_epoch_local(timestamp: int | None) -> str | None:
    if timestamp is None:
        return None
    return time.strftime("%Y-%m-%d %H:%M:%S %z", time.localtime(timestamp))


def campaign_root(repo_root: Path, artifacts_root: Path, campaign_id: str) -> Path:
    return artifacts_root / campaign_id


def ceodex_root(repo_root: Path, artifacts_root: Path, campaign_id: str) -> Path:
    return campaign_root(repo_root, artifacts_root, campaign_id) / "campaign" / "ceodex"


def logs_root(repo_root: Path, artifacts_root: Path, campaign_id: str) -> Path:
    return ceodex_root(repo_root, artifacts_root, campaign_id) / "logs"


def status_json(repo_root: Path, campaign_id: str, artifacts_root: Path) -> dict:
    cmd = [
        str(repo_root / "evals/single_thread/perf_campaign.sh"),
        "status",
        "--campaign-id",
        campaign_id,
        "--artifacts-root",
        str(artifacts_root),
        "--json",
        "--show-items",
    ]
    completed = run(cmd, cwd=repo_root, capture=True)
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr.strip() or completed.stdout.strip() or "status failed")
    return json.loads(completed.stdout)


def active_queue_count(status: dict) -> int:
    active = 0
    for item in status.get("queue_items", []):
        if item.get("status") not in TERMINAL_CANDIDATE_STATUSES:
            active += 1
    return active


def active_queue_candidate_ids(status: dict) -> set[str]:
    active: set[str] = set()
    for item in status.get("queue_items", []):
        if item.get("status") in TERMINAL_CANDIDATE_STATUSES:
            continue
        candidate_id = item.get("candidate_id")
        if candidate_id:
            active.add(candidate_id)
    return active


def candidate_json_paths(repo_root: Path, artifacts_root: Path, campaign_id: str) -> list[Path]:
    candidates_dir = campaign_root(repo_root, artifacts_root, campaign_id) / "candidates"
    if not candidates_dir.is_dir():
        return []
    return sorted(candidates_dir.glob("*/candidate.json"))


def read_candidate(path: Path) -> dict:
    return load_json(path)


def candidate_json_path(repo_root: Path, artifacts_root: Path, campaign_id: str, candidate_id: str) -> Path:
    return campaign_root(repo_root, artifacts_root, campaign_id) / "candidates" / candidate_id / "candidate.json"


def write_candidate(path: Path, candidate: dict) -> None:
    path.write_text(json.dumps(candidate, indent=2, sort_keys=True) + "\n")


def candidate_head(repo_root: Path, candidate: dict) -> str:
    worktree = Path(candidate["candidate_worktree"]).resolve()
    completed = run(["git", "rev-parse", "HEAD"], cwd=worktree, capture=True)
    if completed.returncode != 0:
        return ""
    return completed.stdout.strip()


def candidate_is_clean(candidate: dict) -> bool:
    worktree = Path(candidate["candidate_worktree"]).resolve()
    completed = run(["git", "status", "--short"], cwd=worktree, capture=True)
    return completed.returncode == 0 and completed.stdout.strip() == ""


def git_rev_exists(repo_root: Path, rev: str) -> bool:
    completed = run(["git", "rev-parse", "--verify", f"{rev}^{{commit}}"], cwd=repo_root, capture=True)
    return completed.returncode == 0


def git_is_ancestor(repo_root: Path, older: str, newer: str) -> bool:
    completed = run(["git", "merge-base", "--is-ancestor", older, newer], cwd=repo_root, capture=True)
    return completed.returncode == 0


def parse_iso_timestamp(value: str | None) -> float | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()
    except ValueError:
        return None


def extract_rate_limit_windows(snapshot: dict) -> list[dict]:
    rate_limits = snapshot.get("rateLimits") or {}
    windows: list[dict] = []
    for window_name in ("primary", "secondary"):
        window = rate_limits.get(window_name)
        if not isinstance(window, dict):
            continue
        resets_at = window.get("resetsAt")
        used_percent = window.get("usedPercent")
        try:
            normalized_resets_at = int(resets_at) if resets_at is not None else None
        except (TypeError, ValueError):
            normalized_resets_at = None
        try:
            normalized_used_percent = float(used_percent) if used_percent is not None else None
        except (TypeError, ValueError):
            normalized_used_percent = None
        windows.append(
            {
                "name": window_name,
                "used_percent": normalized_used_percent,
                "window_duration_mins": window.get("windowDurationMins"),
                "resets_at": normalized_resets_at,
                "resets_at_local": format_epoch_local(normalized_resets_at),
            }
        )
    return windows


def exhausted_rate_limit_windows(snapshot: dict) -> list[dict]:
    exhausted: list[dict] = []
    for window in extract_rate_limit_windows(snapshot):
        used_percent = window.get("used_percent")
        resets_at = window.get("resets_at")
        if used_percent is None or used_percent < 100.0 or resets_at is None:
            continue
        exhausted.append(window)
    return exhausted


def build_prompt(
    *,
    repo_root: Path,
    artifacts_root: Path,
    campaign_id: str,
    candidate_id: str,
    topic: str,
    producer_name: str,
    stale_replay: dict | None = None,
) -> str:
    stale_context = ""
    stale_workflow = ""
    if stale_replay is not None:
        stale_context = f"""
Stale replay context:
- This candidate is a replay of stale candidate `{stale_replay['source_candidate_id']}`.
- Original description: `{stale_replay['source_description']}`
- Original topic: `{stale_replay['source_topic']}`
- Original baseline snapshot: `{stale_replay['source_baseline_commit']}`
- Original submitted commit: `{stale_replay['source_commit']}`
- Original branch: `{stale_replay['source_branch']}`
- Replay status before you started: `{stale_replay['replay_mode']}`
"""
        stale_workflow = f"""
Replay-specific requirements:
- Start by inspecting the old change with `git show {stale_replay['source_commit']} --stat -- crates/fastcore/src`.
- Treat this as a mandatory stale resubmission attempt: carry the same optimization idea onto the current baseline if it still exists.
- If a pre-applied cherry-pick left conflicts, resolve them instead of discarding the attempt.
- If the original diff is now mostly absorbed by the current baseline, strengthen the same direction instead of submitting a near-empty replay.
- Only abandon the replay if the optimization idea genuinely disappears or is clearly no longer meaningful on top of the new baseline.
"""
    return f"""You are a direct candidate worker for CEOdex on campaign `{campaign_id}`.

Context:
- Work only in the current git worktree.
- Campaign repo root: `{repo_root}`
- Campaign artifacts root: `{artifacts_root}`
- Candidate id: `{candidate_id}`
- Topic hint: `{topic}`
- Producer lane: `{producer_name}`
{stale_context}

Primary goal:
- Improve single-CPU playout throughput in `crates/fastcore/src/**` without changing deterministic results.

Hard constraints:
- Do not edit evaluator files under `evals/single_thread/**`.
- Do not edit campaign metadata, queue files, ledger files, or results files directly.
- Do not spawn subagents.
- Do not install dependencies.
- If a direction recently measured at least +2% but less than +5%, double down on that direction instead of pivoting away.
- Be bold inside `crates/fastcore/src/**`: large refactors, rewrites, or replacing an approach from scratch are allowed if they plausibly improve throughput and preserve correctness.
- Use the fixed correctness+benchmark harness as the gold standard and optimize fearlessly within the mutable surface.

Required workflow:
1. Read enough context from `program.md`, `evals/single_thread/README.md`, and the hot code you plan to change.
2. Choose one plausible optimization idea related to `{topic}`.
3. Modify only `crates/fastcore/src/**`.
4. Run at least one local sanity check that compiles the touched code. Prefer a targeted `cargo build` or `cargo test`.
5. Commit exactly one candidate commit with a short message.
6. Submit the candidate with:
   `{repo_root}/evals/single_thread/perf_campaign.sh submit-candidate --campaign-id {campaign_id} --candidate-id {candidate_id} --artifacts-root {artifacts_root} --description "<short description>"`
7. In the final message, state the description you submitted.
{stale_workflow}
"""


class CEOdexSupervisor:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.repo_root = Path(args.repo_root).resolve()
        self.artifacts_root = Path(args.artifacts_root).resolve() if args.artifacts_root else (
            self.repo_root / "evals/artifacts/perf_campaigns"
        )
        self.ceodex_dir = ceodex_root(self.repo_root, self.artifacts_root, args.campaign_id)
        self.log_dir = logs_root(self.repo_root, self.artifacts_root, args.campaign_id)
        self.log_dir.mkdir(parents=True, exist_ok=True)
        self.stop_event = threading.Event()
        self.restart_scheduled = threading.Event()
        self.pid_file = self.ceodex_dir / "supervisor.pid"
        self.heartbeat_path = self.ceodex_dir / "heartbeat.json"
        self.rate_limit_log_path = self.ceodex_dir / "rate_limits.jsonl"
        self.rate_limit_event_path = self.ceodex_dir / "rate_limit_events.jsonl"
        self.restart_state_path = self.ceodex_dir / "restart_state.json"
        self.topic_lock = threading.Lock()
        self.restart_lock = threading.Lock()
        self.topic_index = 0
        self.print_lock = threading.Lock()

    def log(self, message: str) -> None:
        timestamp = time.strftime("%Y-%m-%d %H:%M:%S")
        line = f"[{timestamp}] {message}"
        with self.print_lock:
            print(line, flush=True)

    def next_topic(self) -> str:
        with self.topic_lock:
            topic = TOPICS[self.topic_index % len(TOPICS)]
            self.topic_index += 1
            return topic

    def claim_stale_replay(self, producer_name: str) -> dict | None:
        with self.topic_lock:
            now_ts = time.time()
            candidates: list[tuple[str, Path, dict, str]] = []
            for candidate_path in candidate_json_paths(self.repo_root, self.artifacts_root, self.args.campaign_id):
                candidate = read_candidate(candidate_path)
                if candidate.get("status") != "stale":
                    continue
                if candidate.get("stale_replay_child_id"):
                    continue
                claimed_at_ts = parse_iso_timestamp(candidate.get("stale_replay_claimed_at"))
                if claimed_at_ts is not None and now_ts - claimed_at_ts < STALE_REPLAY_CLAIM_TTL_SECONDS:
                    continue
                source_rev = candidate.get("last_submitted_commit")
                if not source_rev:
                    branch_name = candidate.get("candidate_branch")
                    if branch_name and git_rev_exists(self.repo_root, branch_name):
                        source_rev = branch_name
                if not source_rev or not git_rev_exists(self.repo_root, source_rev):
                    candidate["stale_replay_skipped_at"] = now_iso()
                    candidate["stale_replay_skip_reason"] = "missing source commit for stale replay"
                    candidate["updated_at"] = now_iso()
                    write_candidate(candidate_path, candidate)
                    continue
                candidates.append((candidate.get("updated_at", ""), candidate_path, candidate, source_rev))
            if not candidates:
                return None
            candidates.sort(key=lambda row: row[0])
            _, candidate_path, source_candidate, source_rev = candidates[0]
            source_candidate["stale_replay_claimed_at"] = now_iso()
            source_candidate["stale_replay_claimed_by"] = producer_name
            source_candidate["updated_at"] = now_iso()
            write_candidate(candidate_path, source_candidate)
            return {
                "source_candidate_path": candidate_path,
                "source_candidate": source_candidate,
                "source_commit": source_rev,
            }

    def double_down_topic(self) -> str | None:
        status = status_json(self.repo_root, self.args.campaign_id, self.artifacts_root)
        promising: list[tuple[str, str]] = []
        for item in status.get("queue_items", []):
            if item.get("status") != "discard":
                continue
            median = item.get("median_speedup_pct")
            if median is None:
                continue
            try:
                median_value = float(median)
            except (TypeError, ValueError):
                continue
            if median_value < 2.0 or median_value >= 5.0:
                continue
            candidate_id = item.get("candidate_id")
            if not candidate_id:
                continue
            candidate_path = candidate_json_path(
                self.repo_root,
                self.artifacts_root,
                self.args.campaign_id,
                candidate_id,
            )
            if not candidate_path.exists():
                continue
            candidate = read_candidate(candidate_path)
            topic = candidate.get("topic")
            if not topic:
                continue
            last_timestamp = ""
            history = item.get("history") or []
            if history:
                last_timestamp = history[-1].get("timestamp", "")
            promising.append((last_timestamp, str(topic)))
        if not promising:
            return None
        promising.sort()
        return promising[-1][1]

    def write_pid(self) -> None:
        self.pid_file.write_text(f"{os.getpid()}\n")

    def remove_pid(self) -> None:
        try:
            if self.pid_file.exists():
                self.pid_file.unlink()
        except OSError:
            pass

    def write_heartbeat(self) -> None:
        payload = {
            "pid": os.getpid(),
            "campaign_id": self.args.campaign_id,
            "timestamp": int(time.time()),
            "timestamp_local": format_epoch_local(int(time.time())),
            "restart_scheduled": self.restart_scheduled.is_set(),
        }
        self.heartbeat_path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")

    def clear_restart_state(self) -> None:
        try:
            if self.restart_state_path.exists():
                self.restart_state_path.unlink()
        except OSError:
            pass

    def probe_rate_limits(self, source: str) -> dict | None:
        cmd = [sys.executable, str(self.repo_root / "scripts/codex_rate_limit_probe.py")]
        completed = run(cmd, cwd=self.repo_root, capture=True)
        if completed.returncode != 0:
            message = completed.stderr.strip() or completed.stdout.strip() or "rate-limit probe failed"
            self.log(f"{source} rate-limit probe failed: {message}")
            return None
        try:
            snapshot = json.loads(completed.stdout)
        except json.JSONDecodeError as exc:
            self.log(f"{source} rate-limit probe returned invalid JSON: {exc}")
            return None
        record = {
            "captured_at": snapshot.get("capturedAt"),
            "captured_at_local": snapshot.get("capturedAtLocal"),
            "source": source,
            "rateLimits": snapshot.get("rateLimits"),
            "windows": extract_rate_limit_windows(snapshot),
        }
        append_jsonl(self.rate_limit_log_path, record)
        return snapshot

    def schedule_restart_for_rate_limit(
        self,
        *,
        source: str,
        reason: str,
        snapshot: dict,
        output_excerpt: str | None = None,
    ) -> None:
        exhausted = exhausted_rate_limit_windows(snapshot)
        if not exhausted:
            return
        with self.restart_lock:
            if self.restart_scheduled.is_set():
                return
            self.restart_scheduled.set()
            reset_at = max(window["resets_at"] for window in exhausted if window["resets_at"] is not None)
            restart_after = int(reset_at) + self.args.restart_delay_seconds
            state = {
                "campaign_id": self.args.campaign_id,
                "reason": reason,
                "source": source,
                "scheduled_at": int(time.time()),
                "scheduled_at_local": format_epoch_local(int(time.time())),
                "restart_delay_seconds": self.args.restart_delay_seconds,
                "reset_at": reset_at,
                "reset_at_local": format_epoch_local(reset_at),
                "restart_after": restart_after,
                "restart_after_local": format_epoch_local(restart_after),
                "windows": exhausted,
                "output_excerpt": output_excerpt,
            }
            self.restart_state_path.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n")
            append_jsonl(self.rate_limit_event_path, state)
            self.log(
                "rate limit reached via "
                f"{source}; scheduling restart at {state['restart_after_local']} "
                f"(reset {state['reset_at_local']})"
            )
            self.stop_event.set()

    def codex_output_indicates_rate_limit(self, output: str) -> bool:
        lowered = output.lower()
        return any(needle in lowered for needle in RATE_LIMIT_NEEDLE_STRINGS)

    def maybe_schedule_restart_from_snapshot(self, *, source: str, reason: str, snapshot: dict | None) -> None:
        if snapshot is None:
            return
        exhausted = exhausted_rate_limit_windows(snapshot)
        if exhausted:
            self.schedule_restart_for_rate_limit(source=source, reason=reason, snapshot=snapshot)

    def rate_limit_loop(self) -> None:
        while not self.stop_event.is_set():
            snapshot = self.probe_rate_limits("poll")
            self.maybe_schedule_restart_from_snapshot(
                source="poll",
                reason="Codex rate-limit window exhausted during periodic poll",
                snapshot=snapshot,
            )
            deadline = time.monotonic() + self.args.rate_limit_poll_seconds
            while not self.stop_event.is_set() and time.monotonic() < deadline:
                time.sleep(1.0)

    def worker_loop(self) -> None:
        log_path = self.log_dir / "worker.log"
        while not self.stop_event.is_set():
            cmd = [
                str(self.repo_root / "evals/single_thread/perf_campaign.sh"),
                "worker-run",
                "--campaign-id",
                self.args.campaign_id,
                "--artifacts-root",
                str(self.artifacts_root),
                "--drain",
                "--continuous",
                "--auto-advance",
                "--poll-seconds",
                str(self.args.worker_poll_seconds),
                "--worker-id",
                "ceodex-worker",
            ]
            self.log(f"starting worker: {' '.join(cmd)}")
            with log_path.open("a") as handle:
                process = subprocess.Popen(
                    cmd,
                    cwd=str(self.repo_root),
                    stdout=handle,
                    stderr=subprocess.STDOUT,
                    text=True,
                )
                while process.poll() is None and not self.stop_event.is_set():
                    time.sleep(2.0)
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=10)
                    return
                self.log(f"worker exited with code {process.returncode}; restarting in 5s")
            if not self.stop_event.is_set():
                time.sleep(5.0)

    def create_candidate(self, producer_name: str, topic: str) -> dict:
        cmd = [
            str(self.repo_root / "evals/single_thread/perf_campaign.sh"),
            "create-candidate",
            "--campaign-id",
            self.args.campaign_id,
            "--artifacts-root",
            str(self.artifacts_root),
            "--agent-id",
            producer_name,
            "--topic",
            topic,
            "--description",
            topic,
        ]
        completed = run(cmd, cwd=self.repo_root, capture=True)
        if completed.returncode != 0:
            raise RuntimeError(completed.stderr.strip() or completed.stdout.strip() or "create-candidate failed")
        values = {}
        for line in completed.stdout.splitlines():
            if "=" in line:
                key, value = line.split("=", 1)
                values[key.strip()] = value.strip()
        required = {"candidate_id", "worktree"}
        if not required.issubset(values):
            raise RuntimeError(f"unexpected create-candidate output: {completed.stdout}")
        candidate_path = (
            campaign_root(self.repo_root, self.artifacts_root, self.args.campaign_id)
            / "candidates"
            / values["candidate_id"]
            / "candidate.json"
        )
        return read_candidate(candidate_path)

    def prepare_stale_replay(
        self,
        *,
        producer_name: str,
        source_candidate_path: Path,
        source_candidate: dict,
        source_commit: str,
        replay_candidate: dict,
    ) -> dict:
        source_candidate = read_candidate(source_candidate_path)
        baseline_commit = replay_candidate["baseline_commit_snapshot"]
        replay_candidate_path = candidate_json_path(
            self.repo_root,
            self.artifacts_root,
            self.args.campaign_id,
            replay_candidate["candidate_id"],
        )
        replay_candidate["stale_replay_source_candidate_id"] = source_candidate["candidate_id"]
        replay_candidate["stale_replay_source_commit"] = source_commit
        replay_candidate["stale_replay_source_branch"] = source_candidate.get("candidate_branch", "")
        replay_candidate["stale_replay_source_description"] = source_candidate.get("description", "")
        replay_candidate["stale_replay_source_topic"] = source_candidate.get("topic", "")
        replay_candidate["stale_replay_source_baseline_commit"] = source_candidate.get("baseline_commit_snapshot", "")
        write_candidate(replay_candidate_path, replay_candidate)

        replay_mode = "manual-port"
        if git_is_ancestor(self.repo_root, source_commit, baseline_commit):
            source_candidate["stale_replay_child_id"] = replay_candidate["candidate_id"]
            source_candidate["stale_replay_result"] = "absorbed_by_baseline"
            source_candidate["stale_replay_rebased_at"] = now_iso()
            source_candidate["updated_at"] = now_iso()
            write_candidate(source_candidate_path, source_candidate)
            return {
                "source_candidate_id": source_candidate["candidate_id"],
                "source_description": source_candidate.get("description", ""),
                "source_topic": source_candidate.get("topic", ""),
                "source_baseline_commit": source_candidate.get("baseline_commit_snapshot", ""),
                "source_commit": source_commit,
                "source_branch": source_candidate.get("candidate_branch", ""),
                "replay_mode": "absorbed-by-baseline",
            }

        worktree = Path(replay_candidate["candidate_worktree"]).resolve()
        cherry_pick = run(["git", "cherry-pick", "--no-commit", source_commit], cwd=worktree, capture=True)
        if cherry_pick.returncode == 0:
            replay_mode = "preapplied-clean"
        else:
            replay_mode = "preapplied-conflicts"
        replay_candidate["stale_replay_mode"] = replay_mode
        replay_candidate["stale_replay_setup_stdout"] = cherry_pick.stdout[-4000:]
        replay_candidate["stale_replay_setup_stderr"] = cherry_pick.stderr[-4000:]
        write_candidate(replay_candidate_path, replay_candidate)

        source_candidate["stale_replay_child_id"] = replay_candidate["candidate_id"]
        source_candidate["stale_replay_result"] = replay_mode
        source_candidate["stale_replay_rebased_at"] = now_iso()
        source_candidate["updated_at"] = now_iso()
        write_candidate(source_candidate_path, source_candidate)
        self.log(
            f"{producer_name} preparing stale replay {source_candidate['candidate_id']} -> "
            f"{replay_candidate['candidate_id']} mode={replay_mode}"
        )
        return {
            "source_candidate_id": source_candidate["candidate_id"],
            "source_description": source_candidate.get("description", ""),
            "source_topic": source_candidate.get("topic", ""),
            "source_baseline_commit": source_candidate.get("baseline_commit_snapshot", ""),
            "source_commit": source_commit,
            "source_branch": source_candidate.get("candidate_branch", ""),
            "replay_mode": replay_mode,
        }

    def maybe_submit_or_cleanup(self, candidate: dict, producer_name: str, log_handle) -> None:
        refreshed = read_candidate(
            campaign_root(self.repo_root, self.artifacts_root, self.args.campaign_id)
            / "candidates"
            / candidate["candidate_id"]
            / "candidate.json"
        )
        status = refreshed.get("status", "")
        if status not in {"created", ""}:
            self.log(f"{producer_name} finished candidate {candidate['candidate_id']} with status {status}")
            return

        head = candidate_head(self.repo_root, refreshed)
        baseline = refreshed.get("baseline_commit_snapshot", "")
        if head and head != baseline and candidate_is_clean(refreshed):
            description = refreshed.get("description") or refreshed.get("topic") or "ceodex auto-submit"
            submit_cmd = [
                str(self.repo_root / "evals/single_thread/perf_campaign.sh"),
                "submit-candidate",
                "--campaign-id",
                self.args.campaign_id,
                "--artifacts-root",
                str(self.artifacts_root),
                "--candidate-id",
                refreshed["candidate_id"],
                "--description",
                description,
            ]
            self.log(f"{producer_name} auto-submitting committed candidate {refreshed['candidate_id']}")
            completed = run(submit_cmd, cwd=self.repo_root, capture=True)
            log_handle.write(completed.stdout)
            log_handle.write(completed.stderr)
            log_handle.flush()
            return

        cleanup_cmd = [
            str(self.repo_root / "evals/single_thread/perf_campaign.sh"),
            "cleanup-candidate",
            "--campaign-id",
            self.args.campaign_id,
            "--artifacts-root",
            str(self.artifacts_root),
            "--candidate-id",
            refreshed["candidate_id"],
            "--force",
        ]
        self.log(f"{producer_name} cleaning up unsubmitted candidate {refreshed['candidate_id']}")
        completed = run(cleanup_cmd, cwd=self.repo_root, capture=True)
        log_handle.write(completed.stdout)
        log_handle.write(completed.stderr)
        log_handle.flush()

    def run_codex_exec(
        self,
        *,
        codex_cmd: list[str],
        prompt: str,
        producer_name: str,
        candidate_id: str,
        handle,
    ) -> tuple[int, str]:
        output_parts: list[str] = []
        timed_out = False
        process = subprocess.Popen(
            codex_cmd,
            cwd=str(self.repo_root),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
        try:
            assert process.stdin is not None
            process.stdin.write(prompt)
            process.stdin.close()
            assert process.stdout is not None
            deadline = time.monotonic() + self.args.codex_timeout_seconds
            while True:
                if self.stop_event.is_set() and process.poll() is None:
                    self.log(f"{producer_name} stopping codex for {candidate_id}")
                    process.terminate()
                if process.poll() is not None:
                    remainder = process.stdout.read() or ""
                    if remainder:
                        output_parts.append(remainder)
                        handle.write(remainder)
                        handle.flush()
                    break
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    timed_out = True
                    self.log(f"{producer_name} timed out on {candidate_id}; terminating codex")
                    process.terminate()
                    continue
                ready, _, _ = select.select([process.stdout], [], [], min(0.5, remaining))
                if not ready:
                    continue
                chunk = process.stdout.readline()
                if not chunk:
                    continue
                output_parts.append(chunk)
                handle.write(chunk)
                handle.flush()
            try:
                return_code = process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                return_code = process.wait(timeout=10)
        finally:
            if process.stdout is not None:
                process.stdout.close()
            if process.stdin is not None and not process.stdin.closed:
                process.stdin.close()
        output = "".join(output_parts)
        if timed_out:
            return_code = return_code if return_code is not None else 124
        if self.codex_output_indicates_rate_limit(output):
            snapshot = self.probe_rate_limits(f"{producer_name}-usage-limit")
            if snapshot is not None:
                self.schedule_restart_for_rate_limit(
                    source=f"{producer_name}-usage-limit",
                    reason="Codex exec reported a usage/rate limit error",
                    snapshot=snapshot,
                    output_excerpt="\n".join(output.splitlines()[-20:]),
                )
        elif return_code != 0:
            snapshot = self.probe_rate_limits(f"{producer_name}-after-error")
            self.maybe_schedule_restart_from_snapshot(
                source=f"{producer_name}-after-error",
                reason=f"Codex exec exited {return_code} with an exhausted quota window",
                snapshot=snapshot,
            )
        return return_code, output

    def producer_loop(self, lane_index: int) -> None:
        producer_name = f"ceodex-{lane_index:02d}"
        log_path = self.log_dir / f"{producer_name}.log"
        while not self.stop_event.is_set():
            try:
                while not self.stop_event.is_set():
                    status = status_json(self.repo_root, self.args.campaign_id, self.artifacts_root)
                    if active_queue_count(status) < self.args.queue_target:
                        break
                    time.sleep(self.args.producer_poll_seconds)
                if self.stop_event.is_set():
                    return

                stale_replay = self.claim_stale_replay(producer_name)
                snapshot = self.probe_rate_limits(f"{producer_name}-preflight")
                self.maybe_schedule_restart_from_snapshot(
                    source=f"{producer_name}-preflight",
                    reason="Skipped candidate generation because Codex quota is already exhausted",
                    snapshot=snapshot,
                )
                if self.stop_event.is_set():
                    return

                replay_context = None
                if stale_replay is not None:
                    source_candidate = stale_replay["source_candidate"]
                    topic = str(source_candidate.get("topic") or source_candidate.get("description") or self.next_topic())
                    self.log(f"{producer_name} selected stale replay {source_candidate['candidate_id']} on topic {topic}")
                else:
                    topic = self.double_down_topic() or self.next_topic()
                if stale_replay is None and topic in TOPICS:
                    self.log(f"{producer_name} selected topic {topic}")
                candidate = self.create_candidate(producer_name, topic)
                if stale_replay is not None:
                    replay_context = self.prepare_stale_replay(
                        producer_name=producer_name,
                        source_candidate_path=stale_replay["source_candidate_path"],
                        source_candidate=stale_replay["source_candidate"],
                        source_commit=stale_replay["source_commit"],
                        replay_candidate=candidate,
                    )
                    if replay_context["replay_mode"] == "absorbed-by-baseline":
                        with log_path.open("a") as handle:
                            handle.write(
                                f"\n===== {time.strftime('%Y-%m-%d %H:%M:%S')} {candidate['candidate_id']} {topic} =====\n"
                            )
                            handle.write(
                                f"Replay skipped because source commit {replay_context['source_commit']} "
                                "is already in the current baseline.\n"
                            )
                            handle.flush()
                            self.maybe_submit_or_cleanup(candidate, producer_name, handle)
                        continue
                prompt = build_prompt(
                    repo_root=self.repo_root,
                    artifacts_root=self.artifacts_root,
                    campaign_id=self.args.campaign_id,
                    candidate_id=candidate["candidate_id"],
                    topic=topic,
                    producer_name=producer_name,
                    stale_replay=replay_context,
                )
                if replay_context is None:
                    self.log(f"{producer_name} created {candidate['candidate_id']} on topic {topic}")
                else:
                    self.log(
                        f"{producer_name} created replay candidate {candidate['candidate_id']} from "
                        f"{replay_context['source_candidate_id']} mode={replay_context['replay_mode']}"
                    )
                codex_cmd = [
                    "codex",
                    "exec",
                    "--dangerously-bypass-approvals-and-sandbox",
                    "--skip-git-repo-check",
                    "--color",
                    self.args.codex_color,
                    "--cd",
                    str(Path(candidate["candidate_worktree"]).resolve()),
                    "--add-dir",
                    str(self.repo_root),
                    "--output-last-message",
                    str(
                        campaign_root(self.repo_root, self.artifacts_root, self.args.campaign_id)
                        / "candidates"
                        / candidate["candidate_id"]
                        / "codex_last_message.txt"
                    ),
                    "-",
                ]
                if self.args.codex_model:
                    codex_cmd.extend(["--model", self.args.codex_model])
                with log_path.open("a") as handle:
                    handle.write(
                        f"\n===== {time.strftime('%Y-%m-%d %H:%M:%S')} {candidate['candidate_id']} {topic} =====\n"
                    )
                    handle.flush()
                    self.run_codex_exec(
                        codex_cmd=codex_cmd,
                        prompt=prompt,
                        producer_name=producer_name,
                        candidate_id=candidate["candidate_id"],
                        handle=handle,
                    )
                    self.maybe_submit_or_cleanup(candidate, producer_name, handle)
            except Exception as exc:
                self.log(f"{producer_name} error: {exc}; retrying in 10s")
                time.sleep(10.0)

    def cleanup_loop(self) -> None:
        while not self.stop_event.is_set():
            try:
                active_candidates = active_queue_candidate_ids(
                    status_json(self.repo_root, self.args.campaign_id, self.artifacts_root)
                )
                for candidate_path in candidate_json_paths(self.repo_root, self.artifacts_root, self.args.campaign_id):
                    candidate = read_candidate(candidate_path)
                    if candidate.get("worktree_removed"):
                        continue
                    if candidate.get("status") not in TERMINAL_CANDIDATE_STATUSES:
                        continue
                    if candidate["candidate_id"] in active_candidates:
                        self.log(
                            f"cleanup deferred for {candidate['candidate_id']}: active queue item still present"
                        )
                        continue
                    cmd = [
                        str(self.repo_root / "evals/single_thread/perf_campaign.sh"),
                        "cleanup-candidate",
                        "--campaign-id",
                        self.args.campaign_id,
                        "--artifacts-root",
                        str(self.artifacts_root),
                        "--candidate-id",
                        candidate["candidate_id"],
                    ]
                    completed = run(cmd, cwd=self.repo_root, capture=True)
                    if completed.returncode == 0:
                        self.log(f"cleaned {candidate['candidate_id']} status={candidate.get('status')}")
                deadline = time.monotonic() + self.args.cleanup_interval_seconds
                while not self.stop_event.is_set() and time.monotonic() < deadline:
                    time.sleep(1.0)
            except Exception as exc:
                self.log(f"cleanup loop error: {exc}; retrying in 30s")
                time.sleep(30.0)

    def handle_signal(self, signum, _frame) -> None:
        self.log(f"received signal {signum}; stopping")
        self.stop_event.set()

    def run(self) -> int:
        self.clear_restart_state()
        self.write_pid()
        self.write_heartbeat()
        signal.signal(signal.SIGINT, self.handle_signal)
        signal.signal(signal.SIGTERM, self.handle_signal)
        threads = [threading.Thread(target=self.worker_loop, name="worker", daemon=True)]
        threads.extend(
            threading.Thread(target=self.producer_loop, args=(idx + 1,), name=f"producer-{idx+1}", daemon=True)
            for idx in range(self.args.producer_count)
        )
        threads.append(threading.Thread(target=self.cleanup_loop, name="cleanup", daemon=True))
        threads.append(threading.Thread(target=self.rate_limit_loop, name="rate-limit", daemon=True))
        for thread in threads:
            thread.start()
        try:
            while not self.stop_event.is_set():
                self.write_heartbeat()
                time.sleep(self.args.heartbeat_seconds)
        finally:
            self.stop_event.set()
            for thread in threads:
                thread.join(timeout=15.0)
            self.remove_pid()
        return 0


def main() -> int:
    args = parse_args()
    supervisor = CEOdexSupervisor(args)
    return supervisor.run()


if __name__ == "__main__":
    raise SystemExit(main())
