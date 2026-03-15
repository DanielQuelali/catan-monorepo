#!/usr/bin/env python3
import argparse
import fcntl
import hashlib
import json
import os
import re
import signal
import socket
import subprocess
import sys
import time
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
QUEUE_SCHEMA_VERSION = 1
CAMPAIGN_STATUS_ACTIVE = "active"
CAMPAIGN_STATUS_CLOSED = "closed"
TERMINAL_QUEUE_STATES = {
    "keep",
    "discard",
    "build_fail",
    "correctness_fail",
    "benchmark_fail",
    "policy_fail",
    "stale",
    "cancelled",
}
IMMUTABLE_EVALUATOR_FILES = [
    "evals/single_thread/generate_golden.sh",
    "evals/single_thread/verify_against_golden.sh",
    "evals/single_thread/correctness_manifest.json",
    "evals/single_thread/run_correctness_suite.sh",
    "evals/single_thread/benchmark_manifest.json",
    "evals/single_thread/benchmark_candidate.sh",
    "evals/single_thread/benchmark_pair.sh",
    "evals/single_thread/benchmark_compare.py",
]
DEFAULT_MUTABLE_PREFIXES = [
    "crates/fastcore/src/",
]


class CampaignError(RuntimeError):
    pass


@dataclass
class CampaignPaths:
    root: Path
    campaign_dir: Path
    frozen_dir: Path
    queue_dir: Path
    queue_file: Path
    queue_lock: Path
    benchmark_lock: Path
    ledger_dir: Path
    ledger_file: Path
    candidates_dir: Path
    baseline_dir: Path
    metadata_file: Path
    baseline_history_file: Path
    results_tsv_file: Path
    analysis_notebook_file: Path


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def slug(value: str, default: str) -> str:
    normalized = re.sub(r"[^a-zA-Z0-9._-]+", "-", value).strip("-_.").lower()
    return normalized or default


def run(
    cmd: list[str],
    cwd: Path,
    check: bool = True,
    capture: bool = False,
    text: bool = True,
    timeout_sec: float | None = None,
) -> subprocess.CompletedProcess:
    print("+", " ".join(cmd), file=sys.stderr)
    stdout_pipe = subprocess.PIPE if capture else None
    stderr_pipe = subprocess.PIPE if capture else None
    process = subprocess.Popen(
        cmd,
        cwd=cwd,
        stdout=stdout_pipe,
        stderr=stderr_pipe,
        text=text,
        start_new_session=True,
    )
    try:
        stdout, stderr = process.communicate(timeout=timeout_sec)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            stdout, stderr = process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            stdout, stderr = process.communicate()
        raise CampaignError(
            f"Command timed out after {timeout_sec:.1f}s: {' '.join(cmd)}"
        )

    completed = subprocess.CompletedProcess(
        args=cmd,
        returncode=process.returncode,
        stdout=stdout,
        stderr=stderr,
    )
    if check and completed.returncode != 0:
        raise subprocess.CalledProcessError(
            completed.returncode,
            cmd,
            output=completed.stdout,
            stderr=completed.stderr,
        )
    return completed


def git(repo_root: Path, *args: str, capture: bool = False, check: bool = True) -> subprocess.CompletedProcess:
    return run(["git", *args], cwd=repo_root, capture=capture, check=check)


def git_stdout(repo_root: Path, *args: str) -> str:
    completed = git(repo_root, *args, capture=True)
    return completed.stdout.strip()


def load_json(path: Path, *, required: bool = True, default: Any = None) -> Any:
    if not path.exists():
        if required:
            raise CampaignError(f"Missing file: {path}")
        return default
    return json.loads(path.read_text())


def atomic_write_text(path: Path, data: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f".{path.name}.tmp.{os.getpid()}")
    tmp.write_text(data)
    tmp.replace(path)


def atomic_write_json(path: Path, payload: Any) -> None:
    atomic_write_text(path, json.dumps(payload, indent=2, sort_keys=True) + "\n")


def append_jsonl(path: Path, row: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a") as handle:
        handle.write(json.dumps(row, sort_keys=True) + "\n")


def ensure_results_tsv(path: Path) -> None:
    headers = [
        "commit",
        "median_speedup_pct",
        "playouts_per_cpu_second",
        "status",
        "description",
    ]
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists():
        path.write_text("\t".join(headers) + "\n")


def ensure_analysis_notebook(path: Path, template: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists():
        path.write_text(template.read_text())


def ensure_campaign_presentation_assets(paths: CampaignPaths, metadata: dict[str, Any]) -> None:
    ensure_results_tsv(paths.results_tsv_file)
    template = (Path(metadata["repo_root"]).resolve() / "evals/single_thread/analysis.ipynb").resolve()
    if not template.is_file():
        raise CampaignError(f"Missing analysis notebook template: {template}")
    ensure_analysis_notebook(paths.analysis_notebook_file, template)


def effective_float_metadata(metadata: dict[str, Any], key: str, default: float) -> float:
    value = metadata.get(key)
    if value is None:
        return default
    return float(value)


def append_results_tsv(path: Path, row: dict[str, Any]) -> None:
    ensure_results_tsv(path)
    values = [
        str(row.get("commit", "")),
        str(row.get("median_speedup_pct", "")),
        str(row.get("playouts_per_cpu_second", "")),
        str(row.get("status", "")),
        str(row.get("description", "")),
    ]
    with path.open("a") as handle:
        handle.write("\t".join(values) + "\n")


def sync_results_tsv_from_ledger(paths: CampaignPaths) -> None:
    ensure_results_tsv(paths.results_tsv_file)
    existing_lines = paths.results_tsv_file.read_text().splitlines()
    if len(existing_lines) > 1:
        return
    if not paths.ledger_file.exists():
        return
    for line in paths.ledger_file.read_text().splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        append_results_tsv(
            paths.results_tsv_file,
            {
                "commit": str(row.get("candidate_commit", ""))[:7],
                "median_speedup_pct": (
                    f"{float(row.get('median_speedup_pct')):.6f}"
                    if row.get("median_speedup_pct") is not None
                    else "0.000000"
                ),
                "playouts_per_cpu_second": (
                    f"{float(row.get('playouts_per_cpu_second')):.2f}"
                    if row.get("playouts_per_cpu_second") is not None
                    else "0.00"
                ),
                "status": row.get("decision_status", ""),
                "description": row.get("description", ""),
            },
        )


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(64 * 1024)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


@contextmanager
def file_lock(path: Path, *, nonblocking: bool) -> Any:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a+") as handle:
        flags = fcntl.LOCK_EX | (fcntl.LOCK_NB if nonblocking else 0)
        try:
            fcntl.flock(handle.fileno(), flags)
        except BlockingIOError as exc:
            raise CampaignError(f"Lock is already held: {path}") from exc
        try:
            yield handle
        finally:
            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)


def campaign_paths(artifacts_root: Path, campaign_id: str) -> CampaignPaths:
    root = artifacts_root / campaign_id
    return CampaignPaths(
        root=root,
        campaign_dir=root / "campaign",
        frozen_dir=root / "campaign" / "frozen",
        queue_dir=root / "queue",
        queue_file=root / "queue" / "items.json",
        queue_lock=root / "queue" / "queue.lock",
        benchmark_lock=root / "queue" / "benchmark_lane.lock",
        ledger_dir=root / "ledger",
        ledger_file=root / "ledger" / "experiments.jsonl",
        candidates_dir=root / "candidates",
        baseline_dir=root / "baseline",
        metadata_file=root / "campaign" / "metadata.json",
        baseline_history_file=root / "campaign" / "baseline_history.jsonl",
        results_tsv_file=root / "campaign" / "results.tsv",
        analysis_notebook_file=root / "campaign" / "analysis.ipynb",
    )


def ensure_queue(paths: CampaignPaths) -> dict[str, Any]:
    queue = load_json(paths.queue_file, required=False, default=None)
    if queue is None:
        queue = {"schema_version": QUEUE_SCHEMA_VERSION, "items": []}
        atomic_write_json(paths.queue_file, queue)
    if queue.get("schema_version") != QUEUE_SCHEMA_VERSION:
        raise CampaignError(
            f"Unsupported queue schema_version in {paths.queue_file}: {queue.get('schema_version')}"
        )
    if not isinstance(queue.get("items"), list):
        raise CampaignError(f"Queue file is malformed: {paths.queue_file}")
    return queue


def load_metadata(paths: CampaignPaths) -> dict[str, Any]:
    metadata = load_json(paths.metadata_file)
    if metadata.get("schema_version") != SCHEMA_VERSION:
        raise CampaignError(
            f"Unsupported metadata schema_version in {paths.metadata_file}: {metadata.get('schema_version')}"
        )
    return metadata


def enforce_campaign_integrity(metadata: dict[str, Any]) -> None:
    repo_root = Path(metadata["repo_root"]).resolve()
    immutable_files = metadata.get("immutable_evaluator_files", [])
    expected_hashes = metadata.get("immutable_evaluator_hashes", {})
    drifted: list[str] = []
    missing: list[str] = []

    for relative in immutable_files:
        path = repo_root / relative
        if not path.exists():
            missing.append(relative)
            continue
        expected = expected_hashes.get(relative)
        if expected:
            actual = sha256_file(path)
            if actual != expected:
                drifted.append(relative)

    if missing:
        raise CampaignError(
            "Immutable evaluator files are missing: " + ", ".join(sorted(missing))
        )
    if drifted:
        raise CampaignError(
            "Immutable evaluator files changed during active campaign: "
            + ", ".join(sorted(drifted))
        )

    frozen_hash_checks = [
        ("benchmark_manifest", "frozen_benchmark_manifest_sha256"),
        ("correctness_manifest", "frozen_correctness_manifest_sha256"),
    ]
    for path_key, hash_key in frozen_hash_checks:
        if path_key not in metadata:
            continue
        path = Path(metadata[path_key]).resolve()
        if not path.exists():
            raise CampaignError(f"Frozen manifest missing: {path}")
        expected = metadata.get(hash_key)
        if expected and sha256_file(path) != expected:
            raise CampaignError(f"Frozen manifest drift detected: {path}")


def enforce_worker_host(metadata: dict[str, Any], *, allow_mismatch: bool) -> None:
    expected = str(metadata.get("benchmark_host", "")).strip()
    if not expected or allow_mismatch:
        return
    actual = socket.gethostname()
    if actual != expected:
        raise CampaignError(
            f"Worker host mismatch: expected {expected}, got {actual}. "
            "Use --allow-host-mismatch only for deliberate override."
        )


def changed_files_between(repo_root: Path, base: str, head: str) -> list[str]:
    out = git_stdout(repo_root, "diff", "--name-only", f"{base}..{head}")
    return [line.strip() for line in out.splitlines() if line.strip()]


def changed_files_in_worktree(repo_root: Path) -> list[str]:
    out = git_stdout(repo_root, "status", "--porcelain")
    changed = []
    for line in out.splitlines():
        text = line.rstrip()
        if not text:
            continue
        if len(text) > 3:
            changed.append(text[3:])
    return changed


def is_ancestor(repo_root: Path, older: str, newer: str) -> bool:
    completed = git(repo_root, "merge-base", "--is-ancestor", older, newer, check=False)
    return completed.returncode == 0


def read_candidate(paths: CampaignPaths, candidate_id: str) -> dict[str, Any]:
    candidate_file = paths.candidates_dir / candidate_id / "candidate.json"
    return load_json(candidate_file)


def write_candidate(paths: CampaignPaths, candidate: dict[str, Any]) -> None:
    candidate_id = candidate["candidate_id"]
    candidate_file = paths.candidates_dir / candidate_id / "candidate.json"
    atomic_write_json(candidate_file, candidate)


def ledger_row_base(
    metadata: dict[str, Any],
    item: dict[str, Any],
    *,
    correctness_status: str,
    benchmark_status: str,
    decision_status: str,
) -> dict[str, Any]:
    return {
        "timestamp": now_iso(),
        "campaign_id": metadata["campaign_id"],
        "candidate_id": item["candidate_id"],
        "candidate_branch": item["candidate_branch"],
        "candidate_commit": item["candidate_commit"],
        "baseline_commit": item["baseline_commit_snapshot"],
        "baseline_branch": metadata["baseline_branch"],
        "correctness_status": correctness_status,
        "benchmark_status": benchmark_status,
        "decision_status": decision_status,
        "median_speedup_pct": item.get("median_speedup_pct"),
        "min_speedup_pct": item.get("min_speedup_pct"),
        "max_slowdown_pct": item.get("max_slowdown_pct"),
        "cpu_ns_per_playout": item.get("cpu_ns_per_playout"),
        "playouts_per_cpu_second": item.get("playouts_per_cpu_second"),
        "total_playouts_per_repeat": item.get("total_playouts_per_repeat"),
        "total_paired_playouts_all_repeats": item.get("total_paired_playouts_all_repeats"),
        "artifact_root": item.get("artifact_root"),
        "description": item.get("description", ""),
        "state": item.get("status"),
    }


def enqueue_item(queue: dict[str, Any], item: dict[str, Any]) -> None:
    queue["items"].append(item)


def transition_item(item: dict[str, Any], status: str, note: str) -> None:
    item["status"] = status
    history = item.setdefault("history", [])
    history.append(
        {
            "timestamp": now_iso(),
            "status": status,
            "note": note,
        }
    )


def first_queue_item(queue: dict[str, Any]) -> dict[str, Any] | None:
    candidates = [
        item
        for item in queue["items"]
        if item["status"] in {"queued", "queued_for_benchmark"}
    ]
    if not candidates:
        return None
    candidates.sort(key=lambda row: row.get("enqueue_timestamp", ""))
    return candidates[0]


def recover_inflight_states(queue: dict[str, Any]) -> bool:
    changed = False
    for item in queue["items"]:
        if item["status"] == "running_correctness":
            transition_item(item, "queued", "Recovered running_correctness after restart")
            changed = True
        elif item["status"] == "running_benchmark":
            transition_item(item, "queued_for_benchmark", "Recovered running_benchmark after restart")
            changed = True
    return changed


def parse_args() -> argparse.Namespace:
    repo_root = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(
        description="Campaign controller for single-CPU autoresearch-style optimization."
    )
    parser.set_defaults(repo_root=repo_root)
    sub = parser.add_subparsers(dest="command", required=True)

    init_cmd = sub.add_parser("init", help="Initialize a new campaign")
    init_cmd.add_argument("--campaign-id", required=True)
    init_cmd.add_argument(
        "--artifacts-root",
        default=str(repo_root / "evals/artifacts/perf_campaigns"),
    )
    init_cmd.add_argument("--baseline-ref", default="HEAD")
    init_cmd.add_argument("--baseline-branch", default=None)
    init_cmd.add_argument("--benchmark-suite", default="gate")
    init_cmd.add_argument("--correctness-suite", default="gate")
    init_cmd.add_argument("--benchmark-repeats", type=int, default=5)
    init_cmd.add_argument("--threshold-pct", type=float, default=5.0)
    init_cmd.add_argument("--critical-regression-limit-pct", type=float, default=-1.0)
    init_cmd.add_argument("--correctness-timeout-sec", type=float, default=1800.0)
    init_cmd.add_argument("--benchmark-timeout-sec", type=float, default=3600.0)
    init_cmd.add_argument(
        "--benchmark-manifest",
        default=str(repo_root / "evals/single_thread/benchmark_manifest.json"),
    )
    init_cmd.add_argument(
        "--correctness-manifest",
        default=str(repo_root / "evals/single_thread/correctness_manifest.json"),
    )
    init_cmd.add_argument("--baseline-worktree", default=None)
    init_cmd.add_argument("--golden-dir", default=None)
    init_cmd.add_argument("--benchmark-host", default=socket.gethostname())
    init_cmd.add_argument("--description", default="")
    init_cmd.add_argument("--generate-goldens", action="store_true")
    init_cmd.add_argument(
        "--mutable-prefix",
        action="append",
        default=[],
        help="Additional mutable path prefix; may be provided more than once.",
    )

    status_cmd = sub.add_parser("status", help="Inspect campaign metadata and queue")
    status_cmd.add_argument("--campaign-id", required=True)
    status_cmd.add_argument(
        "--artifacts-root",
        default=str(repo_root / "evals/artifacts/perf_campaigns"),
    )
    status_cmd.add_argument("--json", action="store_true")
    status_cmd.add_argument("--show-items", action="store_true")

    results_cmd = sub.add_parser("results", help="Show autoresearch-style TSV results summary")
    results_cmd.add_argument("--campaign-id", required=True)
    results_cmd.add_argument(
        "--artifacts-root",
        default=str(repo_root / "evals/artifacts/perf_campaigns"),
    )
    results_cmd.add_argument("--json", action="store_true")

    create_cmd = sub.add_parser("create-candidate", help="Create a candidate branch and worktree")
    create_cmd.add_argument("--campaign-id", required=True)
    create_cmd.add_argument(
        "--artifacts-root",
        default=str(repo_root / "evals/artifacts/perf_campaigns"),
    )
    create_cmd.add_argument("--agent-id", required=True)
    create_cmd.add_argument("--topic", required=True)
    create_cmd.add_argument("--description", default="")
    create_cmd.add_argument("--branch", default=None)
    create_cmd.add_argument("--worktree", default=None)

    submit_cmd = sub.add_parser("submit-candidate", help="Preflight and enqueue a candidate")
    submit_cmd.add_argument("--campaign-id", required=True)
    submit_cmd.add_argument(
        "--artifacts-root",
        default=str(repo_root / "evals/artifacts/perf_campaigns"),
    )
    submit_cmd.add_argument("--candidate-id", required=True)
    submit_cmd.add_argument("--description", default=None)
    submit_cmd.add_argument("--allow-outside-mutable-surface", action="store_true")
    submit_cmd.add_argument(
        "--outside-mutable-justification",
        default="",
        help="Required when --allow-outside-mutable-surface is set and out-of-surface files are changed",
    )

    worker_cmd = sub.add_parser("worker-run", help="Drain queue through correctness + benchmark lane")
    worker_cmd.add_argument("--campaign-id", required=True)
    worker_cmd.add_argument(
        "--artifacts-root",
        default=str(repo_root / "evals/artifacts/perf_campaigns"),
    )
    worker_cmd.add_argument("--drain", action="store_true")
    worker_cmd.add_argument("--auto-advance", action="store_true")
    worker_cmd.add_argument("--worker-id", default=f"{socket.gethostname()}:{os.getpid()}")
    worker_cmd.add_argument("--allow-host-mismatch", action="store_true")
    worker_cmd.add_argument("--continuous", action="store_true")
    worker_cmd.add_argument("--poll-seconds", type=float, default=15.0)
    worker_cmd.add_argument(
        "--max-idle-cycles",
        type=int,
        default=0,
        help="Only for --continuous. 0 means run forever.",
    )

    advance_cmd = sub.add_parser("advance-baseline", help="Advance campaign baseline to a kept candidate")
    advance_cmd.add_argument("--campaign-id", required=True)
    advance_cmd.add_argument(
        "--artifacts-root",
        default=str(repo_root / "evals/artifacts/perf_campaigns"),
    )
    advance_cmd.add_argument("--candidate-id", required=True)
    advance_cmd.add_argument("--reason", default="")
    advance_cmd.add_argument("--no-mark-stale", action="store_true")

    cleanup_cmd = sub.add_parser("cleanup-candidate", help="Remove candidate worktree after terminal decision")
    cleanup_cmd.add_argument("--campaign-id", required=True)
    cleanup_cmd.add_argument(
        "--artifacts-root",
        default=str(repo_root / "evals/artifacts/perf_campaigns"),
    )
    cleanup_cmd.add_argument("--candidate-id", required=True)
    cleanup_cmd.add_argument("--force", action="store_true")

    cancel_cmd = sub.add_parser("cancel-candidate", help="Cancel a queued or running candidate")
    cancel_cmd.add_argument("--campaign-id", required=True)
    cancel_cmd.add_argument(
        "--artifacts-root",
        default=str(repo_root / "evals/artifacts/perf_campaigns"),
    )
    cancel_cmd.add_argument("--candidate-id", required=True)
    cancel_cmd.add_argument("--reason", default="cancelled by operator")

    return parser.parse_args()


def command_init(args: argparse.Namespace) -> int:
    repo_root = Path(args.repo_root).resolve()
    artifacts_root = Path(args.artifacts_root).resolve()
    campaign_id = slug(args.campaign_id, "campaign")
    baseline_branch = args.baseline_branch or f"perf-campaign-{campaign_id}-baseline"
    baseline_ref = args.baseline_ref
    benchmark_manifest = Path(args.benchmark_manifest).resolve()
    correctness_manifest = Path(args.correctness_manifest).resolve()
    analysis_template = (repo_root / "evals/single_thread/analysis.ipynb").resolve()
    baseline_worktree = Path(
        args.baseline_worktree or f"/tmp/catan-perf-{campaign_id}-baseline"
    ).resolve()

    paths = campaign_paths(artifacts_root, campaign_id)
    if paths.root.exists():
        raise CampaignError(f"Campaign already exists: {paths.root}")
    if not benchmark_manifest.is_file():
        raise CampaignError(f"Missing benchmark manifest: {benchmark_manifest}")
    if not correctness_manifest.is_file():
        raise CampaignError(f"Missing correctness manifest: {correctness_manifest}")
    if not analysis_template.is_file():
        raise CampaignError(f"Missing analysis notebook template: {analysis_template}")
    if args.correctness_timeout_sec <= 0:
        raise CampaignError("--correctness-timeout-sec must be > 0")
    if args.benchmark_timeout_sec <= 0:
        raise CampaignError("--benchmark-timeout-sec must be > 0")

    baseline_commit = git_stdout(repo_root, "rev-parse", baseline_ref)
    branch_exists = (
        git(
            repo_root,
            "show-ref",
            "--verify",
            "--quiet",
            f"refs/heads/{baseline_branch}",
            check=False,
        ).returncode
        == 0
    )
    if branch_exists:
        existing_commit = git_stdout(repo_root, "rev-parse", baseline_branch)
        if existing_commit != baseline_commit:
            raise CampaignError(
                f"Baseline branch {baseline_branch} already exists at {existing_commit}, expected {baseline_commit}"
            )
    else:
        git(repo_root, "branch", baseline_branch, baseline_commit)

    if baseline_worktree.exists():
        if not (baseline_worktree / ".git").exists():
            raise CampaignError(f"Baseline worktree path exists but is not a git worktree: {baseline_worktree}")
        current_branch = git_stdout(baseline_worktree, "rev-parse", "--abbrev-ref", "HEAD")
        if current_branch != baseline_branch:
            raise CampaignError(
                f"Baseline worktree already exists on branch {current_branch}, expected {baseline_branch}"
            )
        current_commit = git_stdout(baseline_worktree, "rev-parse", "HEAD")
        if current_commit != baseline_commit:
            raise CampaignError(
                f"Baseline worktree commit {current_commit} does not match baseline {baseline_commit}"
            )
    else:
        git(repo_root, "worktree", "add", str(baseline_worktree), baseline_branch)

    for directory in [
        paths.campaign_dir,
        paths.frozen_dir,
        paths.queue_dir,
        paths.ledger_dir,
        paths.candidates_dir,
        paths.baseline_dir,
    ]:
        directory.mkdir(parents=True, exist_ok=True)

    frozen_benchmark_manifest = paths.frozen_dir / "benchmark_manifest.json"
    frozen_correctness_manifest = paths.frozen_dir / "correctness_manifest.json"
    frozen_benchmark_manifest.write_text(benchmark_manifest.read_text())
    frozen_correctness_manifest.write_text(correctness_manifest.read_text())
    frozen_benchmark_manifest_sha256 = sha256_file(frozen_benchmark_manifest)
    frozen_correctness_manifest_sha256 = sha256_file(frozen_correctness_manifest)

    evaluator_hashes = {}
    for relative in IMMUTABLE_EVALUATOR_FILES:
        path = repo_root / relative
        if not path.exists():
            raise CampaignError(f"Immutable evaluator file missing: {path}")
        evaluator_hashes[relative] = sha256_file(path)

    mutable_prefixes = sorted(set(DEFAULT_MUTABLE_PREFIXES + list(args.mutable_prefix)))
    golden_dir = Path(args.golden_dir).resolve() if args.golden_dir else (paths.root / "goldens" / args.correctness_suite)
    metadata = {
        "schema_version": SCHEMA_VERSION,
        "campaign_id": campaign_id,
        "status": CAMPAIGN_STATUS_ACTIVE,
        "created_at": now_iso(),
        "description": args.description,
        "repo_root": str(repo_root),
        "artifacts_root": str(paths.root),
        "benchmark_host": args.benchmark_host,
        "baseline_ref_input": baseline_ref,
        "baseline_branch": baseline_branch,
        "baseline_commit": baseline_commit,
        "baseline_worktree": str(baseline_worktree),
        "benchmark_suite": args.benchmark_suite,
        "correctness_suite": args.correctness_suite,
        "benchmark_repeats": int(args.benchmark_repeats),
        "threshold_pct": float(args.threshold_pct),
        "critical_regression_limit_pct": float(args.critical_regression_limit_pct),
        "correctness_timeout_sec": float(args.correctness_timeout_sec),
        "benchmark_timeout_sec": float(args.benchmark_timeout_sec),
        "benchmark_manifest": str(frozen_benchmark_manifest),
        "correctness_manifest": str(frozen_correctness_manifest),
        "golden_dir": str(golden_dir),
        "mutable_prefixes": mutable_prefixes,
        "immutable_evaluator_files": IMMUTABLE_EVALUATOR_FILES,
        "immutable_evaluator_hashes": evaluator_hashes,
        "frozen_benchmark_manifest_sha256": frozen_benchmark_manifest_sha256,
        "frozen_correctness_manifest_sha256": frozen_correctness_manifest_sha256,
    }
    atomic_write_json(paths.metadata_file, metadata)
    atomic_write_json(paths.queue_file, {"schema_version": QUEUE_SCHEMA_VERSION, "items": []})
    ensure_results_tsv(paths.results_tsv_file)
    ensure_analysis_notebook(paths.analysis_notebook_file, analysis_template)
    append_jsonl(
        paths.baseline_history_file,
        {
            "timestamp": now_iso(),
            "event": "initialize",
            "campaign_id": campaign_id,
            "baseline_branch": baseline_branch,
            "baseline_commit": baseline_commit,
            "note": "campaign initialized",
        },
    )

    if args.generate_goldens:
        run(
            [
                str(repo_root / "evals/single_thread/run_correctness_suite.sh"),
                "generate",
                "--root",
                str(baseline_worktree),
                "--manifest",
                str(frozen_correctness_manifest),
                "--suite",
                args.correctness_suite,
                "--out-dir",
                str(golden_dir),
            ],
            cwd=repo_root,
        )

    print(f"Initialized campaign {campaign_id}")
    print(f"Campaign root: {paths.root}")
    print(f"Baseline branch: {baseline_branch}")
    print(f"Baseline worktree: {baseline_worktree}")
    print(f"Golden dir: {golden_dir}")
    print(f"Results TSV: {paths.results_tsv_file}")
    print(f"Analysis notebook: {paths.analysis_notebook_file}")
    return 0


def command_status(args: argparse.Namespace) -> int:
    paths = campaign_paths(Path(args.artifacts_root).resolve(), args.campaign_id)
    metadata = load_metadata(paths)
    ensure_campaign_presentation_assets(paths, metadata)
    queue = ensure_queue(paths)
    counts: dict[str, int] = {}
    for item in queue["items"]:
        counts[item["status"]] = counts.get(item["status"], 0) + 1
    payload = {
        "campaign_id": metadata["campaign_id"],
        "status": metadata["status"],
        "baseline_branch": metadata["baseline_branch"],
        "baseline_commit": metadata["baseline_commit"],
        "baseline_worktree": metadata["baseline_worktree"],
        "benchmark_suite": metadata["benchmark_suite"],
        "correctness_suite": metadata["correctness_suite"],
        "threshold_pct": metadata["threshold_pct"],
        "critical_regression_limit_pct": metadata["critical_regression_limit_pct"],
        "correctness_timeout_sec": effective_float_metadata(metadata, "correctness_timeout_sec", 1800.0),
        "benchmark_timeout_sec": effective_float_metadata(metadata, "benchmark_timeout_sec", 3600.0),
        "golden_dir": metadata["golden_dir"],
        "results_tsv": str(paths.results_tsv_file),
        "analysis_notebook": str(paths.analysis_notebook_file),
        "queue_counts": counts,
        "queue_items_total": len(queue["items"]),
    }
    if args.show_items:
        payload["queue_items"] = queue["items"]
    if args.json:
        print(json.dumps(payload, indent=2, sort_keys=True))
        return 0

    print(f"campaign_id={payload['campaign_id']}")
    print(f"status={payload['status']}")
    print(f"baseline_branch={payload['baseline_branch']}")
    print(f"baseline_commit={payload['baseline_commit']}")
    print(f"baseline_worktree={payload['baseline_worktree']}")
    print(f"benchmark_suite={payload['benchmark_suite']} correctness_suite={payload['correctness_suite']}")
    print(
        "threshold_pct="
        f"{payload['threshold_pct']} critical_regression_limit_pct={payload['critical_regression_limit_pct']}"
    )
    print(
        "correctness_timeout_sec="
        f"{payload['correctness_timeout_sec']} benchmark_timeout_sec={payload['benchmark_timeout_sec']}"
    )
    print(f"golden_dir={payload['golden_dir']}")
    print(f"results_tsv={payload['results_tsv']}")
    print(f"analysis_notebook={payload['analysis_notebook']}")
    print(f"queue_items_total={payload['queue_items_total']}")
    for status in sorted(payload["queue_counts"]):
        print(f"queue_count_{status}={payload['queue_counts'][status]}")
    return 0


def command_results(args: argparse.Namespace) -> int:
    paths = campaign_paths(Path(args.artifacts_root).resolve(), args.campaign_id)
    metadata = load_metadata(paths)
    ensure_campaign_presentation_assets(paths, metadata)
    sync_results_tsv_from_ledger(paths)
    lines = paths.results_tsv_file.read_text().splitlines()
    header = lines[0].split("\t") if lines else []
    rows = []
    for line in lines[1:]:
        if not line.strip():
            continue
        values = line.split("\t")
        rows.append(dict(zip(header, values)))

    counts: dict[str, int] = {}
    best_keep = None
    for row in rows:
        status = row.get("status", "")
        counts[status] = counts.get(status, 0) + 1
        if status == "keep":
            current_pps = float(row.get("playouts_per_cpu_second", "0") or 0.0)
            if best_keep is None or current_pps > float(best_keep.get("playouts_per_cpu_second", "0") or 0.0):
                best_keep = row

    payload = {
        "campaign_id": args.campaign_id,
        "results_tsv": str(paths.results_tsv_file),
        "analysis_notebook": str(paths.analysis_notebook_file),
        "total_experiments": len(rows),
        "status_counts": counts,
        "best_keep": best_keep,
        "rows": rows if args.json else None,
    }
    if args.json:
        print(json.dumps(payload, indent=2, sort_keys=True))
        return 0

    print(f"campaign_id={payload['campaign_id']}")
    print(f"results_tsv={payload['results_tsv']}")
    print(f"analysis_notebook={payload['analysis_notebook']}")
    print(f"total_experiments={payload['total_experiments']}")
    for status in sorted(counts):
        print(f"status_count_{status}={counts[status]}")
    if best_keep:
        print("best_keep_commit=" + str(best_keep.get("commit", "")))
        print("best_keep_playouts_per_cpu_second=" + str(best_keep.get("playouts_per_cpu_second", "")))
        print("best_keep_median_speedup_pct=" + str(best_keep.get("median_speedup_pct", "")))
        print("best_keep_description=" + str(best_keep.get("description", "")))
    else:
        print("best_keep_commit=")
    return 0


def unique_candidate_id(paths: CampaignPaths, agent_id: str, topic: str) -> str:
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    base = f"{stamp}_{slug(agent_id, 'agent')}_{slug(topic, 'topic')[:24]}"
    candidate_id = base
    seq = 2
    while (paths.candidates_dir / candidate_id).exists():
        candidate_id = f"{base}_{seq}"
        seq += 1
    return candidate_id


def unique_branch_name(repo_root: Path, preferred: str) -> str:
    if (
        git(
            repo_root,
            "show-ref",
            "--verify",
            "--quiet",
            f"refs/heads/{preferred}",
            check=False,
        ).returncode
        != 0
    ):
        return preferred
    seq = 2
    while True:
        candidate = f"{preferred}-{seq}"
        if (
            git(
                repo_root,
                "show-ref",
                "--verify",
                "--quiet",
                f"refs/heads/{candidate}",
                check=False,
            ).returncode
            != 0
        ):
            return candidate
        seq += 1


def command_create_candidate(args: argparse.Namespace) -> int:
    artifacts_root = Path(args.artifacts_root).resolve()
    paths = campaign_paths(artifacts_root, args.campaign_id)
    metadata = load_metadata(paths)
    repo_root = Path(metadata["repo_root"]).resolve()
    enforce_campaign_integrity(metadata)
    if metadata["status"] != CAMPAIGN_STATUS_ACTIVE:
        raise CampaignError(f"Campaign is not active: {metadata['status']}")

    candidate_id = unique_candidate_id(paths, args.agent_id, args.topic)
    agent_slug = slug(args.agent_id, "agent")
    topic_slug = slug(args.topic, "topic")
    default_branch = (
        f"perf-campaign-{metadata['campaign_id']}-{agent_slug}-{topic_slug}-{candidate_id[-8:]}"
    )
    branch_name = unique_branch_name(repo_root, args.branch or default_branch)
    baseline_commit = metadata["baseline_commit"]

    worktree = Path(
        args.worktree or f"/tmp/catan-perf-{metadata['campaign_id']}-{candidate_id}"
    ).resolve()
    if worktree.exists():
        raise CampaignError(f"Candidate worktree path already exists: {worktree}")

    git(repo_root, "branch", branch_name, baseline_commit)
    git(repo_root, "worktree", "add", str(worktree), branch_name)

    candidate_dir = paths.candidates_dir / candidate_id
    candidate_dir.mkdir(parents=True, exist_ok=True)
    candidate = {
        "candidate_id": candidate_id,
        "campaign_id": metadata["campaign_id"],
        "agent_id": args.agent_id,
        "topic": args.topic,
        "description": args.description,
        "created_at": now_iso(),
        "candidate_branch": branch_name,
        "candidate_worktree": str(worktree),
        "baseline_commit_snapshot": baseline_commit,
        "status": "created",
        "artifact_root": str(candidate_dir / "artifacts"),
        "submission_count": 0,
    }
    write_candidate(paths, candidate)
    print(f"candidate_id={candidate_id}")
    print(f"branch={branch_name}")
    print(f"worktree={worktree}")
    print(f"baseline_commit_snapshot={baseline_commit}")
    return 0


def make_preflight_item(
    *,
    metadata: dict[str, Any],
    candidate: dict[str, Any],
    candidate_commit: str,
    description: str,
    changed_files: list[str],
) -> dict[str, Any]:
    return {
        "campaign_id": metadata["campaign_id"],
        "candidate_id": candidate["candidate_id"],
        "candidate_branch": candidate["candidate_branch"],
        "candidate_commit": candidate_commit,
        "baseline_commit_snapshot": candidate["baseline_commit_snapshot"],
        "candidate_worktree": candidate["candidate_worktree"],
        "status": "queued",
        "description": description,
        "enqueue_timestamp": now_iso(),
        "artifact_root": candidate["artifact_root"],
        "changed_files": changed_files,
        "history": [
            {
                "timestamp": now_iso(),
                "status": "queued",
                "note": "submitted",
            }
        ],
    }


def command_submit_candidate(args: argparse.Namespace) -> int:
    artifacts_root = Path(args.artifacts_root).resolve()
    paths = campaign_paths(artifacts_root, args.campaign_id)
    metadata = load_metadata(paths)
    candidate = read_candidate(paths, args.candidate_id)
    repo_root = Path(metadata["repo_root"]).resolve()
    candidate_root = Path(candidate["candidate_worktree"]).resolve()
    enforce_campaign_integrity(metadata)

    if metadata["status"] != CAMPAIGN_STATUS_ACTIVE:
        raise CampaignError(f"Campaign is not active: {metadata['status']}")
    if not candidate_root.exists():
        raise CampaignError(f"Candidate worktree does not exist: {candidate_root}")

    with file_lock(paths.queue_lock, nonblocking=False):
        queue = ensure_queue(paths)
        existing = [
            item
            for item in queue["items"]
            if item["candidate_id"] == candidate["candidate_id"]
            and item["status"] not in TERMINAL_QUEUE_STATES
        ]
        if existing:
            raise CampaignError(
                f"Candidate {candidate['candidate_id']} already has active queue item with status {existing[0]['status']}"
            )

        branch = git_stdout(candidate_root, "rev-parse", "--abbrev-ref", "HEAD")
        if branch != candidate["candidate_branch"]:
            raise CampaignError(
                f"Candidate worktree branch mismatch: expected {candidate['candidate_branch']} got {branch}"
            )
        dirty = changed_files_in_worktree(candidate_root)
        if dirty:
            item = {
                "campaign_id": metadata["campaign_id"],
                "candidate_id": candidate["candidate_id"],
                "candidate_branch": candidate["candidate_branch"],
                "candidate_commit": git_stdout(candidate_root, "rev-parse", "HEAD"),
                "baseline_commit_snapshot": candidate["baseline_commit_snapshot"],
                "candidate_worktree": str(candidate_root),
                "status": "policy_fail",
                "description": args.description if args.description is not None else candidate.get("description", ""),
                "artifact_root": candidate["artifact_root"],
                "dirty_paths": dirty,
            }
            queue["items"].append(item)
            transition_item(item, "policy_fail", "worktree must be clean before submission")
            atomic_write_json(paths.queue_file, queue)
            append_jsonl(
                paths.ledger_file,
                ledger_row_base(
                    metadata,
                    item,
                    correctness_status="skipped",
                    benchmark_status="skipped",
                    decision_status="policy_fail",
                ),
            )
            candidate["status"] = "policy_fail"
            candidate["last_error"] = "dirty worktree"
            candidate["updated_at"] = now_iso()
            write_candidate(paths, candidate)
            print(
                f"Candidate {candidate['candidate_id']} rejected: dirty worktree ({len(dirty)} paths)"
            )
            return 0

        candidate_commit = git_stdout(candidate_root, "rev-parse", "HEAD")
        baseline_snapshot = candidate["baseline_commit_snapshot"]
        active_baseline = metadata["baseline_commit"]
        changed_files = changed_files_between(repo_root, baseline_snapshot, candidate_commit)
        touched_forbidden = sorted(
            set(changed_files).intersection(set(metadata["immutable_evaluator_files"]))
        )
        if touched_forbidden:
            item = {
                "campaign_id": metadata["campaign_id"],
                "candidate_id": candidate["candidate_id"],
                "candidate_branch": candidate["candidate_branch"],
                "candidate_commit": candidate_commit,
                "baseline_commit_snapshot": baseline_snapshot,
                "candidate_worktree": str(candidate_root),
                "status": "policy_fail",
                "description": args.description if args.description is not None else candidate.get("description", ""),
                "artifact_root": candidate["artifact_root"],
                "forbidden_files": touched_forbidden,
            }
            transition_item(item, "policy_fail", "candidate touched immutable evaluator files")
            queue["items"].append(item)
            atomic_write_json(paths.queue_file, queue)
            append_jsonl(
                paths.ledger_file,
                ledger_row_base(
                    metadata,
                    item,
                    correctness_status="skipped",
                    benchmark_status="skipped",
                    decision_status="policy_fail",
                ),
            )
            candidate["status"] = "policy_fail"
            candidate["updated_at"] = now_iso()
            candidate["last_error"] = "immutable evaluator files changed"
            write_candidate(paths, candidate)
            print(f"Candidate {candidate['candidate_id']} rejected: immutable evaluator files changed")
            return 0

        outside_surface = [
            path
            for path in changed_files
            if not any(path.startswith(prefix) for prefix in metadata["mutable_prefixes"])
        ]
        if outside_surface and args.allow_outside_mutable_surface and not args.outside_mutable_justification.strip():
            item = {
                "campaign_id": metadata["campaign_id"],
                "candidate_id": candidate["candidate_id"],
                "candidate_branch": candidate["candidate_branch"],
                "candidate_commit": candidate_commit,
                "baseline_commit_snapshot": baseline_snapshot,
                "candidate_worktree": str(candidate_root),
                "status": "policy_fail",
                "description": args.description if args.description is not None else candidate.get("description", ""),
                "artifact_root": candidate["artifact_root"],
                "outside_mutable_surface": outside_surface,
            }
            transition_item(
                item,
                "policy_fail",
                "outside mutable surface requires --outside-mutable-justification",
            )
            queue["items"].append(item)
            atomic_write_json(paths.queue_file, queue)
            append_jsonl(
                paths.ledger_file,
                ledger_row_base(
                    metadata,
                    item,
                    correctness_status="skipped",
                    benchmark_status="skipped",
                    decision_status="policy_fail",
                ),
            )
            candidate["status"] = "policy_fail"
            candidate["updated_at"] = now_iso()
            candidate["last_error"] = "outside mutable surface without justification"
            write_candidate(paths, candidate)
            print(
                f"Candidate {candidate['candidate_id']} rejected: outside mutable surface requires justification"
            )
            return 0
        if outside_surface and not args.allow_outside_mutable_surface:
            item = {
                "campaign_id": metadata["campaign_id"],
                "candidate_id": candidate["candidate_id"],
                "candidate_branch": candidate["candidate_branch"],
                "candidate_commit": candidate_commit,
                "baseline_commit_snapshot": baseline_snapshot,
                "candidate_worktree": str(candidate_root),
                "status": "policy_fail",
                "description": args.description if args.description is not None else candidate.get("description", ""),
                "artifact_root": candidate["artifact_root"],
                "outside_mutable_surface": outside_surface,
            }
            transition_item(item, "policy_fail", "candidate touched files outside mutable performance surface")
            queue["items"].append(item)
            atomic_write_json(paths.queue_file, queue)
            append_jsonl(
                paths.ledger_file,
                ledger_row_base(
                    metadata,
                    item,
                    correctness_status="skipped",
                    benchmark_status="skipped",
                    decision_status="policy_fail",
                ),
            )
            candidate["status"] = "policy_fail"
            candidate["updated_at"] = now_iso()
            candidate["last_error"] = "outside mutable surface"
            write_candidate(paths, candidate)
            print(f"Candidate {candidate['candidate_id']} rejected: outside mutable surface")
            return 0

        description = args.description if args.description is not None else candidate.get("description", "")
        item = make_preflight_item(
            metadata=metadata,
            candidate=candidate,
            candidate_commit=candidate_commit,
            description=description,
            changed_files=changed_files,
        )
        if outside_surface:
            item["outside_mutable_surface"] = outside_surface
            item["outside_mutable_justification"] = args.outside_mutable_justification

        if baseline_snapshot != active_baseline:
            transition_item(item, "stale", "candidate baseline snapshot is not current campaign baseline")
            queue["items"].append(item)
            atomic_write_json(paths.queue_file, queue)
            append_jsonl(
                paths.ledger_file,
                ledger_row_base(
                    metadata,
                    item,
                    correctness_status="skipped",
                    benchmark_status="skipped",
                    decision_status="stale",
                ),
            )
            candidate["status"] = "stale"
            candidate["updated_at"] = now_iso()
            candidate["submission_count"] = int(candidate.get("submission_count", 0)) + 1
            write_candidate(paths, candidate)
            print(f"Candidate {candidate['candidate_id']} marked stale at submission time")
            return 0

        queue["items"].append(item)
        atomic_write_json(paths.queue_file, queue)
        candidate["status"] = "queued"
        candidate["updated_at"] = now_iso()
        candidate["submission_count"] = int(candidate.get("submission_count", 0)) + 1
        candidate["last_submitted_commit"] = candidate_commit
        write_candidate(paths, candidate)
        print(f"Candidate {candidate['candidate_id']} enqueued for evaluation")
        return 0


def run_correctness(
    repo_root: Path,
    metadata: dict[str, Any],
    item: dict[str, Any],
) -> None:
    candidate_root = Path(item["candidate_worktree"]).resolve()
    artifact_root = Path(item["artifact_root"]).resolve()
    out_dir = artifact_root / "correctness"
    out_dir.mkdir(parents=True, exist_ok=True)
    run(
        [
            str(repo_root / "evals/single_thread/run_correctness_suite.sh"),
            "verify",
            "--root",
            str(candidate_root),
            "--manifest",
            str(Path(metadata["correctness_manifest"]).resolve()),
            "--suite",
            str(metadata["correctness_suite"]),
            "--golden-dir",
            str(Path(metadata["golden_dir"]).resolve()),
            "--work-dir",
            str(out_dir),
        ],
        cwd=repo_root,
        timeout_sec=effective_float_metadata(metadata, "correctness_timeout_sec", 1800.0),
    )


def run_benchmark_pair(
    repo_root: Path,
    metadata: dict[str, Any],
    item: dict[str, Any],
) -> dict[str, Any]:
    candidate_root = Path(item["candidate_worktree"]).resolve()
    artifact_root = Path(item["artifact_root"]).resolve()
    out_dir = artifact_root / "benchmark_pair"
    out_dir.mkdir(parents=True, exist_ok=True)
    run(
        [
            str(repo_root / "evals/single_thread/benchmark_pair.sh"),
            "--baseline-root",
            str(Path(metadata["baseline_worktree"]).resolve()),
            "--candidate-root",
            str(candidate_root),
            "--manifest",
            str(Path(metadata["benchmark_manifest"]).resolve()),
            "--suite",
            str(metadata["benchmark_suite"]),
            "--correctness-manifest",
            str(Path(metadata["correctness_manifest"]).resolve()),
            "--correctness-suite",
            str(metadata["correctness_suite"]),
            "--golden-dir",
            str(Path(metadata["golden_dir"]).resolve()),
            "--repeats",
            str(metadata["benchmark_repeats"]),
            "--threshold-pct",
            str(metadata["threshold_pct"]),
            "--critical-regression-limit-pct",
            str(metadata["critical_regression_limit_pct"]),
            "--description",
            item.get("description", ""),
            "--out-dir",
            str(out_dir),
            "--skip-correctness",
        ],
        cwd=repo_root,
        timeout_sec=effective_float_metadata(metadata, "benchmark_timeout_sec", 3600.0),
    )
    summary_path = out_dir / "summary.json"
    if not summary_path.exists():
        raise CampaignError(f"Benchmark summary is missing: {summary_path}")
    return load_json(summary_path)


def write_terminal_ledger_row(
    paths: CampaignPaths,
    metadata: dict[str, Any],
    item: dict[str, Any],
    *,
    correctness_status: str,
    benchmark_status: str,
    decision_status: str,
) -> None:
    row = ledger_row_base(
        metadata,
        item,
        correctness_status=correctness_status,
        benchmark_status=benchmark_status,
        decision_status=decision_status,
    )
    append_jsonl(paths.ledger_file, row)
    commit = str(item.get("candidate_commit", ""))[:7]
    median_speedup = item.get("median_speedup_pct")
    pps = item.get("playouts_per_cpu_second")
    append_results_tsv(
        paths.results_tsv_file,
        {
            "commit": commit,
            "median_speedup_pct": (
                f"{float(median_speedup):.6f}" if median_speedup is not None else "0.000000"
            ),
            "playouts_per_cpu_second": (
                f"{float(pps):.2f}" if pps is not None else "0.00"
            ),
            "status": decision_status,
            "description": item.get("description", ""),
        },
    )


def set_candidate_status(paths: CampaignPaths, candidate_id: str, status: str, note: str) -> None:
    candidate = read_candidate(paths, candidate_id)
    candidate["status"] = status
    candidate["updated_at"] = now_iso()
    candidate["last_note"] = note
    write_candidate(paths, candidate)


def command_worker_run(args: argparse.Namespace) -> int:
    artifacts_root = Path(args.artifacts_root).resolve()
    paths = campaign_paths(artifacts_root, args.campaign_id)
    metadata = load_metadata(paths)
    repo_root = Path(metadata["repo_root"]).resolve()
    if metadata["status"] != CAMPAIGN_STATUS_ACTIVE:
        raise CampaignError(f"Campaign is not active: {metadata['status']}")
    enforce_campaign_integrity(metadata)
    enforce_worker_host(metadata, allow_mismatch=args.allow_host_mismatch)
    if args.poll_seconds <= 0:
        raise CampaignError("--poll-seconds must be > 0")
    if args.max_idle_cycles < 0:
        raise CampaignError("--max-idle-cycles must be >= 0")

    with file_lock(paths.benchmark_lock, nonblocking=True):
        with file_lock(paths.queue_lock, nonblocking=False):
            queue = ensure_queue(paths)
            if recover_inflight_states(queue):
                atomic_write_json(paths.queue_file, queue)

        total_processed = 0
        idle_cycles = 0
        while True:
            processed = 0
            while True:
                metadata = load_metadata(paths)
                enforce_campaign_integrity(metadata)
                enforce_worker_host(metadata, allow_mismatch=args.allow_host_mismatch)
                with file_lock(paths.queue_lock, nonblocking=False):
                    queue = ensure_queue(paths)
                    item = first_queue_item(queue)
                    if not item:
                        break
                    if item["baseline_commit_snapshot"] != metadata["baseline_commit"]:
                        transition_item(item, "stale", "baseline advanced before evaluation")
                        atomic_write_json(paths.queue_file, queue)
                        set_candidate_status(paths, item["candidate_id"], "stale", "baseline advanced")
                        write_terminal_ledger_row(
                            paths,
                            metadata,
                            item,
                            correctness_status="skipped",
                            benchmark_status="skipped",
                            decision_status="stale",
                        )
                        processed += 1
                        if not args.drain:
                            break
                        continue

                    phase = "benchmark" if item["status"] == "queued_for_benchmark" else "correctness"
                    if item["status"] == "queued":
                        transition_item(item, "running_correctness", f"worker {args.worker_id} running correctness")
                    elif item["status"] == "queued_for_benchmark":
                        transition_item(item, "running_benchmark", f"worker {args.worker_id} running benchmark")
                    else:
                        continue
                    atomic_write_json(paths.queue_file, queue)
                    item_snapshot = json.loads(json.dumps(item))

                if phase == "correctness":
                    try:
                        run_correctness(repo_root, metadata, item_snapshot)
                    except Exception as exc:
                        with file_lock(paths.queue_lock, nonblocking=False):
                            queue = ensure_queue(paths)
                            target = next(
                                row for row in queue["items"] if row["candidate_id"] == item_snapshot["candidate_id"]
                            )
                            if target["status"] == "cancelled":
                                atomic_write_json(paths.queue_file, queue)
                                processed += 1
                                if not args.drain:
                                    break
                                continue
                            transition_item(target, "correctness_fail", f"correctness failed: {exc}")
                            atomic_write_json(paths.queue_file, queue)
                            set_candidate_status(paths, item_snapshot["candidate_id"], "correctness_fail", str(exc))
                            write_terminal_ledger_row(
                                paths,
                                metadata,
                                target,
                                correctness_status="fail",
                                benchmark_status="skipped",
                                decision_status="correctness_fail",
                            )
                        processed += 1
                        if not args.drain:
                            break
                        continue

                    with file_lock(paths.queue_lock, nonblocking=False):
                        queue = ensure_queue(paths)
                        target = next(
                            row for row in queue["items"] if row["candidate_id"] == item_snapshot["candidate_id"]
                        )
                        if target["status"] == "cancelled":
                            atomic_write_json(paths.queue_file, queue)
                            processed += 1
                            if not args.drain:
                                break
                            continue
                        if target["status"] != "running_correctness":
                            transition_item(
                                target,
                                "benchmark_fail",
                                "queue state changed unexpectedly after correctness phase",
                            )
                            atomic_write_json(paths.queue_file, queue)
                            set_candidate_status(
                                paths,
                                item_snapshot["candidate_id"],
                                "benchmark_fail",
                                "state changed after correctness",
                            )
                            write_terminal_ledger_row(
                                paths,
                                metadata,
                                target,
                                correctness_status="pass",
                                benchmark_status="fail",
                                decision_status="benchmark_fail",
                            )
                            processed += 1
                            if not args.drain:
                                break
                            continue
                        transition_item(target, "queued_for_benchmark", "correctness passed")
                        atomic_write_json(paths.queue_file, queue)
                        item_snapshot = json.loads(json.dumps(target))

                    metadata = load_metadata(paths)
                    enforce_campaign_integrity(metadata)
                    enforce_worker_host(metadata, allow_mismatch=args.allow_host_mismatch)
                    if item_snapshot["baseline_commit_snapshot"] != metadata["baseline_commit"]:
                        with file_lock(paths.queue_lock, nonblocking=False):
                            queue = ensure_queue(paths)
                            target = next(
                                row for row in queue["items"] if row["candidate_id"] == item_snapshot["candidate_id"]
                            )
                            if target["status"] == "cancelled":
                                atomic_write_json(paths.queue_file, queue)
                                processed += 1
                                if not args.drain:
                                    break
                                continue
                            if target["status"] == "queued_for_benchmark":
                                transition_item(target, "stale", "baseline advanced before benchmark")
                                atomic_write_json(paths.queue_file, queue)
                                set_candidate_status(paths, target["candidate_id"], "stale", "baseline advanced")
                                write_terminal_ledger_row(
                                    paths,
                                    metadata,
                                    target,
                                    correctness_status="pass",
                                    benchmark_status="skipped",
                                    decision_status="stale",
                                )
                                processed += 1
                                if not args.drain:
                                    break
                                continue

                    with file_lock(paths.queue_lock, nonblocking=False):
                        queue = ensure_queue(paths)
                        target = next(
                            row for row in queue["items"] if row["candidate_id"] == item_snapshot["candidate_id"]
                        )
                        if target["status"] == "cancelled":
                            atomic_write_json(paths.queue_file, queue)
                            processed += 1
                            if not args.drain:
                                break
                            continue
                        if target["status"] != "queued_for_benchmark":
                            processed += 1
                            if not args.drain:
                                break
                            continue
                        transition_item(target, "running_benchmark", f"worker {args.worker_id} running benchmark")
                        atomic_write_json(paths.queue_file, queue)
                        item_snapshot = json.loads(json.dumps(target))

                try:
                    summary = run_benchmark_pair(repo_root, metadata, item_snapshot)
                except Exception as exc:
                    with file_lock(paths.queue_lock, nonblocking=False):
                        queue = ensure_queue(paths)
                        target = next(
                            row for row in queue["items"] if row["candidate_id"] == item_snapshot["candidate_id"]
                        )
                        if target["status"] == "cancelled":
                            atomic_write_json(paths.queue_file, queue)
                            processed += 1
                            if not args.drain:
                                break
                            continue
                        transition_item(target, "benchmark_fail", f"benchmark failed: {exc}")
                        atomic_write_json(paths.queue_file, queue)
                        set_candidate_status(paths, item_snapshot["candidate_id"], "benchmark_fail", str(exc))
                        write_terminal_ledger_row(
                            paths,
                            metadata,
                            target,
                            correctness_status="pass",
                            benchmark_status="fail",
                            decision_status="benchmark_fail",
                        )
                    processed += 1
                    if not args.drain:
                        break
                    continue

                with file_lock(paths.queue_lock, nonblocking=False):
                    queue = ensure_queue(paths)
                    target = next(
                        row for row in queue["items"] if row["candidate_id"] == item_snapshot["candidate_id"]
                    )
                    if target["status"] == "cancelled":
                        atomic_write_json(paths.queue_file, queue)
                        processed += 1
                        if not args.drain:
                            break
                        continue
                    target["median_speedup_pct"] = summary.get("median_speedup_pct")
                    target["min_speedup_pct"] = summary.get("min_speedup_pct")
                    target["max_slowdown_pct"] = summary.get("max_slowdown_pct")
                    target["cpu_ns_per_playout"] = summary.get("cpu_ns_per_playout")
                    target["playouts_per_cpu_second"] = summary.get("playouts_per_cpu_second")
                    target["total_playouts_per_repeat"] = summary.get("candidate_total_playouts_per_repeat")
                    target["total_paired_playouts_all_repeats"] = summary.get("total_paired_playouts_all_repeats")
                    decision = summary.get("status", "benchmark_fail")
                    if decision not in {"keep", "discard"}:
                        decision = "benchmark_fail"
                    transition_item(target, decision, "benchmark decision from summary.json")
                    target["summary_json"] = str(
                        (Path(target["artifact_root"]).resolve() / "benchmark_pair" / "summary.json")
                    )
                    atomic_write_json(paths.queue_file, queue)
                    set_candidate_status(paths, item_snapshot["candidate_id"], decision, "worker benchmark decision")
                    write_terminal_ledger_row(
                        paths,
                        metadata,
                        target,
                        correctness_status="pass",
                        benchmark_status="pass",
                        decision_status=decision,
                    )
                    auto_advance_candidate_id = (
                        item_snapshot["candidate_id"] if decision == "keep" and args.auto_advance else None
                    )
                if auto_advance_candidate_id:
                    try:
                        faux_args = argparse.Namespace(
                            campaign_id=metadata["campaign_id"],
                            artifacts_root=str(paths.root.parent),
                            candidate_id=auto_advance_candidate_id,
                            reason="auto-advance from worker-run",
                            no_mark_stale=False,
                        )
                        command_advance_baseline(faux_args)
                    except CampaignError as exc:
                        print(
                            f"WARNING: auto-advance failed for {auto_advance_candidate_id}: {exc}",
                            file=sys.stderr,
                        )
                processed += 1
                if not args.drain:
                    break

            total_processed += processed
            if not args.continuous:
                if processed == 0:
                    print("No queued candidates")
                else:
                    print(f"Processed {processed} candidate(s)")
                return 0

            if processed == 0:
                idle_cycles += 1
                print(f"Idle cycle {idle_cycles}: no queued candidates")
                if args.max_idle_cycles and idle_cycles >= args.max_idle_cycles:
                    print(f"Stopping after {idle_cycles} idle cycle(s)")
                    break
                time.sleep(args.poll_seconds)
            else:
                idle_cycles = 0

    print(f"Processed {total_processed} candidate(s) total")
    return 0


def command_advance_baseline(args: argparse.Namespace) -> int:
    artifacts_root = Path(args.artifacts_root).resolve()
    paths = campaign_paths(artifacts_root, args.campaign_id)
    metadata = load_metadata(paths)
    repo_root = Path(metadata["repo_root"]).resolve()
    baseline_worktree = Path(metadata["baseline_worktree"]).resolve()
    if metadata["status"] != CAMPAIGN_STATUS_ACTIVE:
        raise CampaignError(f"Campaign is not active: {metadata['status']}")
    enforce_campaign_integrity(metadata)

    with file_lock(paths.queue_lock, nonblocking=False):
        queue = ensure_queue(paths)
        matches = [item for item in queue["items"] if item["candidate_id"] == args.candidate_id]
        if not matches:
            raise CampaignError(f"No queue item found for candidate {args.candidate_id}")
        item = matches[-1]
        if item["status"] != "keep":
            raise CampaignError(
                f"Candidate {args.candidate_id} is not in keep state (current={item['status']})"
            )
        if item["baseline_commit_snapshot"] != metadata["baseline_commit"]:
            raise CampaignError(
                "Candidate baseline snapshot does not match current baseline; candidate is stale"
            )
        candidate_commit = item["candidate_commit"]
        current_baseline = metadata["baseline_commit"]
        if not is_ancestor(repo_root, current_baseline, candidate_commit):
            raise CampaignError(
                f"Baseline advancement requires fast-forward ancestry: {current_baseline} !<= {candidate_commit}"
            )

        if changed_files_in_worktree(baseline_worktree):
            raise CampaignError(f"Baseline worktree is dirty: {baseline_worktree}")
        run(["git", "merge", "--ff-only", candidate_commit], cwd=baseline_worktree)
        new_baseline = git_stdout(baseline_worktree, "rev-parse", "HEAD")
        metadata["baseline_commit"] = new_baseline
        metadata["baseline_updated_at"] = now_iso()
        atomic_write_json(paths.metadata_file, metadata)
        append_jsonl(
            paths.baseline_history_file,
            {
                "timestamp": now_iso(),
                "event": "advance",
                "campaign_id": metadata["campaign_id"],
                "old_baseline_commit": current_baseline,
                "new_baseline_commit": new_baseline,
                "candidate_id": args.candidate_id,
                "candidate_commit": candidate_commit,
                "reason": args.reason,
            },
        )

        stale_marked = 0
        if not args.no_mark_stale:
            for other in queue["items"]:
                if other["candidate_id"] == args.candidate_id:
                    continue
                if other["status"] in TERMINAL_QUEUE_STATES:
                    continue
                if other["baseline_commit_snapshot"] != new_baseline:
                    transition_item(other, "stale", "baseline advanced")
                    set_candidate_status(paths, other["candidate_id"], "stale", "baseline advanced")
                    write_terminal_ledger_row(
                        paths,
                        metadata,
                        other,
                        correctness_status="skipped",
                        benchmark_status="skipped",
                        decision_status="stale",
                    )
                    stale_marked += 1

        atomic_write_json(paths.queue_file, queue)
        set_candidate_status(paths, args.candidate_id, "accepted", "baseline advanced to candidate commit")
        print(f"Baseline advanced to {new_baseline} from candidate {args.candidate_id}")
        if stale_marked:
            print(f"Marked {stale_marked} queued candidate(s) as stale")
        return 0


def command_cancel_candidate(args: argparse.Namespace) -> int:
    artifacts_root = Path(args.artifacts_root).resolve()
    paths = campaign_paths(artifacts_root, args.campaign_id)
    metadata = load_metadata(paths)
    if metadata["status"] != CAMPAIGN_STATUS_ACTIVE:
        raise CampaignError(f"Campaign is not active: {metadata['status']}")
    enforce_campaign_integrity(metadata)

    with file_lock(paths.queue_lock, nonblocking=False):
        queue = ensure_queue(paths)
        matches = [item for item in queue["items"] if item["candidate_id"] == args.candidate_id]
        if not matches:
            raise CampaignError(f"No queue item found for candidate {args.candidate_id}")
        item = matches[-1]
        if item["status"] in TERMINAL_QUEUE_STATES:
            print(f"Candidate {args.candidate_id} is already terminal ({item['status']})")
            return 0

        transition_item(item, "cancelled", args.reason)
        atomic_write_json(paths.queue_file, queue)
        set_candidate_status(paths, args.candidate_id, "cancelled", args.reason)
        write_terminal_ledger_row(
            paths,
            metadata,
            item,
            correctness_status="skipped",
            benchmark_status="skipped",
            decision_status="cancelled",
        )
    print(f"Candidate {args.candidate_id} cancelled")
    return 0


def command_cleanup_candidate(args: argparse.Namespace) -> int:
    paths = campaign_paths(Path(args.artifacts_root).resolve(), args.campaign_id)
    metadata = load_metadata(paths)
    repo_root = Path(metadata["repo_root"]).resolve()
    candidate = read_candidate(paths, args.candidate_id)
    worktree = Path(candidate["candidate_worktree"]).resolve()

    if not args.force and candidate.get("status") not in TERMINAL_QUEUE_STATES.union({"accepted", "created"}):
        raise CampaignError(
            f"Refusing cleanup for non-terminal candidate status={candidate.get('status')}; use --force to override"
        )
    if worktree.exists():
        cmd = ["git", "worktree", "remove"]
        if args.force:
            cmd.append("--force")
        cmd.append(str(worktree))
        run(cmd, cwd=repo_root)
    candidate["cleaned_at"] = now_iso()
    candidate["updated_at"] = now_iso()
    candidate["worktree_removed"] = True
    write_candidate(paths, candidate)
    print(f"Candidate {args.candidate_id} cleanup complete")
    return 0


def main() -> int:
    args = parse_args()
    command = args.command
    handlers = {
        "init": command_init,
        "status": command_status,
        "results": command_results,
        "create-candidate": command_create_candidate,
        "submit-candidate": command_submit_candidate,
        "worker-run": command_worker_run,
        "advance-baseline": command_advance_baseline,
        "cancel-candidate": command_cancel_candidate,
        "cleanup-candidate": command_cleanup_candidate,
    }
    try:
        return handlers[command](args)
    except CampaignError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2
    except subprocess.CalledProcessError as exc:
        print(f"ERROR: command failed ({exc.returncode}): {' '.join(exc.cmd)}", file=sys.stderr)
        if exc.stdout:
            print(exc.stdout, file=sys.stderr)
        if exc.stderr:
            print(exc.stderr, file=sys.stderr)
        return 3


if __name__ == "__main__":
    raise SystemExit(main())
