#!/usr/bin/env python3
import argparse
import json
import statistics
import subprocess
from datetime import datetime, timezone
from pathlib import Path

DEFAULT_ACCEPTANCE_THRESHOLD_PCT = 5.0
DEFAULT_CRITICAL_REGRESSION_LIMIT_PCT = -1.0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Compare paired benchmark runs.")
    parser.add_argument("--pair-dir", required=True, help="Benchmark pair output directory")
    parser.add_argument("--baseline-root", required=True, help="Baseline repo root")
    parser.add_argument("--candidate-root", required=True, help="Candidate repo root")
    parser.add_argument(
        "--correctness-status",
        default="skipped",
        choices=["pass", "fail", "skipped"],
        help="Correctness gate status for the candidate",
    )
    parser.add_argument(
        "--threshold-pct",
        type=float,
        default=DEFAULT_ACCEPTANCE_THRESHOLD_PCT,
        help="Minimum corpus median speedup to keep a candidate",
    )
    parser.add_argument(
        "--critical-regression-limit-pct",
        type=float,
        default=DEFAULT_CRITICAL_REGRESSION_LIMIT_PCT,
        help="Maximum allowed median slowdown on any critical workload",
    )
    parser.add_argument(
        "--description",
        default="",
        help="Short free-form description for the log row",
    )
    parser.add_argument(
        "--summary-json",
        default=None,
        help="Optional explicit summary JSON path",
    )
    parser.add_argument(
        "--log-tsv",
        default=None,
        help="Optional append-only TSV path",
    )
    return parser.parse_args()


def git_output(root: Path, *args: str) -> str:
    return (
        subprocess.run(
            ["git", *args],
            cwd=root,
            capture_output=True,
            text=True,
            check=True,
        )
        .stdout.strip()
    )


def median(values: list[float]) -> float:
    if not values:
        return 0.0
    return float(statistics.median(values))


def load_json(path: Path) -> dict:
    return json.loads(path.read_text())


def pps(cpu_ns_per_playout: float) -> float:
    if cpu_ns_per_playout <= 0.0:
        return 0.0
    return 1_000_000_000.0 / cpu_ns_per_playout


def append_tsv(path: Path, row: dict) -> None:
    headers = [
        "timestamp",
        "candidate_branch",
        "candidate_commit",
        "base_commit",
        "correctness_status",
        "median_speedup_pct",
        "min_speedup_pct",
        "max_slowdown_pct",
        "baseline_cpu_ns_per_playout",
        "candidate_cpu_ns_per_playout",
        "baseline_playouts_per_cpu_second",
        "candidate_playouts_per_cpu_second",
        "baseline_total_playouts_per_repeat",
        "candidate_total_playouts_per_repeat",
        "total_paired_playouts_all_repeats",
        "status",
        "description",
    ]
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists():
        path.write_text("\t".join(headers) + "\n")
    values = [str(row.get(header, "")) for header in headers]
    with path.open("a") as handle:
        handle.write("\t".join(values) + "\n")


def main() -> int:
    args = parse_args()
    pair_dir = Path(args.pair_dir).resolve()
    summary_path = Path(args.summary_json).resolve() if args.summary_json else pair_dir / "summary.json"
    baseline_root = Path(args.baseline_root).resolve()
    candidate_root = Path(args.candidate_root).resolve()

    repeat_dirs = sorted(path for path in (pair_dir / "repeats").glob("*") if path.is_dir())
    if not repeat_dirs:
        raise RuntimeError(f"No repeat directories found in {pair_dir / 'repeats'}")

    per_workload_pairs: dict[str, list[dict]] = {}
    baseline_metrics: dict[str, list[float]] = {}
    candidate_metrics: dict[str, list[float]] = {}
    invalid_metric_pairs: list[dict] = []
    baseline_run_metrics: list[dict] = []
    candidate_run_metrics: list[dict] = []

    for repeat_dir in repeat_dirs:
        baseline_run = load_json(repeat_dir / "baseline.json")
        candidate_run = load_json(repeat_dir / "candidate.json")
        baseline_run_metrics.append(
            {
                "total_playouts": int(baseline_run.get("total_playouts", 0)),
                "cpu_ns_per_playout": float(baseline_run.get("cpu_ns_per_playout", 0.0)),
                "playouts_per_cpu_second": float(baseline_run.get("playouts_per_cpu_second", 0.0)),
            }
        )
        candidate_run_metrics.append(
            {
                "total_playouts": int(candidate_run.get("total_playouts", 0)),
                "cpu_ns_per_playout": float(candidate_run.get("cpu_ns_per_playout", 0.0)),
                "playouts_per_cpu_second": float(candidate_run.get("playouts_per_cpu_second", 0.0)),
            }
        )
        baseline_workloads = {item["id"]: item for item in baseline_run["workloads"]}
        candidate_workloads = {item["id"]: item for item in candidate_run["workloads"]}
        if baseline_workloads.keys() != candidate_workloads.keys():
            raise RuntimeError(f"Workload mismatch in {repeat_dir}")

        for workload_id in sorted(baseline_workloads):
            base = baseline_workloads[workload_id]
            cand = candidate_workloads[workload_id]
            base_metric = float(base["cpu_ns_per_playout"])
            cand_metric = float(cand["cpu_ns_per_playout"])
            invalid_metric = base_metric <= 0.0 or cand_metric < 0.0
            if invalid_metric:
                improvement = 0.0
                invalid_metric_pairs.append(
                    {
                        "repeat": repeat_dir.name,
                        "workload_id": workload_id,
                        "baseline_cpu_ns_per_playout": base_metric,
                        "candidate_cpu_ns_per_playout": cand_metric,
                    }
                )
            else:
                improvement = ((base_metric - cand_metric) / base_metric) * 100.0
            per_workload_pairs.setdefault(workload_id, []).append(
                {
                    "repeat": repeat_dir.name,
                    "baseline_cpu_ns_per_playout": base_metric,
                    "candidate_cpu_ns_per_playout": cand_metric,
                    "baseline_playouts_per_cpu_second": float(base.get("playouts_per_cpu_second", pps(base_metric))),
                    "candidate_playouts_per_cpu_second": float(cand.get("playouts_per_cpu_second", pps(cand_metric))),
                    "playouts": int(base.get("work_units", 0)),
                    "improvement_pct": improvement,
                    "invalid_metric": invalid_metric,
                    "critical": bool(base.get("critical", False) or cand.get("critical", False)),
                    "kind": cand["kind"],
                    "tags": cand.get("tags", []),
                }
            )
            baseline_metrics.setdefault(workload_id, []).append(base_metric)
            candidate_metrics.setdefault(workload_id, []).append(cand_metric)

    workloads_summary = []
    workload_medians = []
    critical_regressions = []
    for workload_id in sorted(per_workload_pairs):
        pairs = per_workload_pairs[workload_id]
        improvements = [item["improvement_pct"] for item in pairs]
        workload_median = median(improvements)
        workload_medians.append(workload_median)
        critical = any(item["critical"] for item in pairs)
        if critical and workload_median < args.critical_regression_limit_pct:
            critical_regressions.append(
                {
                    "workload_id": workload_id,
                    "median_improvement_pct": workload_median,
                }
            )
        baseline_median_cpu_ns = median(baseline_metrics[workload_id])
        candidate_median_cpu_ns = median(candidate_metrics[workload_id])
        workloads_summary.append(
            {
                "id": workload_id,
                "kind": pairs[0]["kind"],
                "critical": critical,
                "tags": pairs[0]["tags"],
                "pair_count": len(pairs),
                "playouts_per_repeat": pairs[0]["playouts"],
                "median_improvement_pct": workload_median,
                "min_improvement_pct": min(improvements),
                "max_improvement_pct": max(improvements),
                "win_rate": sum(1 for value in improvements if value > 0.0) / len(improvements),
                "baseline_median_cpu_ns_per_playout": baseline_median_cpu_ns,
                "candidate_median_cpu_ns_per_playout": candidate_median_cpu_ns,
                "baseline_median_playouts_per_cpu_second": pps(baseline_median_cpu_ns),
                "candidate_median_playouts_per_cpu_second": pps(candidate_median_cpu_ns),
                "pairs": pairs,
            }
        )

    corpus_median = median(workload_medians)
    min_speedup = min(workload_medians) if workload_medians else 0.0
    max_slowdown = abs(min_speedup) if min_speedup < 0.0 else 0.0
    baseline_cpu_ns = median([item["cpu_ns_per_playout"] for item in baseline_run_metrics])
    candidate_cpu_ns = median([item["cpu_ns_per_playout"] for item in candidate_run_metrics])
    baseline_total_playouts = median([float(item["total_playouts"]) for item in baseline_run_metrics])
    candidate_total_playouts = median([float(item["total_playouts"]) for item in candidate_run_metrics])
    total_paired_playouts_all_repeats = sum(item["total_playouts"] for item in baseline_run_metrics) + sum(
        item["total_playouts"] for item in candidate_run_metrics
    )

    if args.correctness_status == "fail":
        status = "correctness_fail"
    elif invalid_metric_pairs:
        status = "benchmark_fail"
    elif critical_regressions:
        status = "discard"
    elif corpus_median > args.threshold_pct:
        status = "keep"
    else:
        status = "discard"

    baseline_commit = git_output(baseline_root, "rev-parse", "HEAD")
    candidate_commit = git_output(candidate_root, "rev-parse", "HEAD")
    candidate_branch = git_output(candidate_root, "rev-parse", "--abbrev-ref", "HEAD")
    timestamp = datetime.now(timezone.utc).isoformat()

    summary = {
        "format": "single-cpu-benchmark-summary-v2",
        "timestamp": timestamp,
        "pair_dir": str(pair_dir),
        "baseline_root": str(baseline_root),
        "candidate_root": str(candidate_root),
        "base_commit": baseline_commit,
        "candidate_commit": candidate_commit,
        "candidate_branch": candidate_branch,
        "correctness_status": args.correctness_status,
        "threshold_pct": args.threshold_pct,
        "critical_regression_limit_pct": args.critical_regression_limit_pct,
        "median_speedup_pct": corpus_median,
        "min_speedup_pct": min_speedup,
        "max_slowdown_pct": max_slowdown,
        "baseline_cpu_ns_per_playout": baseline_cpu_ns,
        "candidate_cpu_ns_per_playout": candidate_cpu_ns,
        "cpu_ns_per_playout": candidate_cpu_ns,
        "baseline_playouts_per_cpu_second": pps(baseline_cpu_ns),
        "candidate_playouts_per_cpu_second": pps(candidate_cpu_ns),
        "playouts_per_cpu_second": pps(candidate_cpu_ns),
        "baseline_total_playouts_per_repeat": int(baseline_total_playouts),
        "candidate_total_playouts_per_repeat": int(candidate_total_playouts),
        "total_paired_playouts_all_repeats": int(total_paired_playouts_all_repeats),
        "repeat_count": len(repeat_dirs),
        "status": status,
        "description": args.description,
        "critical_regressions": critical_regressions,
        "invalid_metric_pairs": invalid_metric_pairs,
        "workloads": workloads_summary,
    }
    summary_path.parent.mkdir(parents=True, exist_ok=True)
    summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")

    if args.log_tsv:
        append_tsv(
            Path(args.log_tsv).resolve(),
            {
                "timestamp": timestamp,
                "candidate_branch": candidate_branch,
                "candidate_commit": candidate_commit,
                "base_commit": baseline_commit,
                "correctness_status": args.correctness_status,
                "median_speedup_pct": f"{corpus_median:.4f}",
                "min_speedup_pct": f"{min_speedup:.4f}",
                "max_slowdown_pct": f"{max_slowdown:.4f}",
                "baseline_cpu_ns_per_playout": f"{baseline_cpu_ns:.2f}",
                "candidate_cpu_ns_per_playout": f"{candidate_cpu_ns:.2f}",
                "baseline_playouts_per_cpu_second": f"{pps(baseline_cpu_ns):.2f}",
                "candidate_playouts_per_cpu_second": f"{pps(candidate_cpu_ns):.2f}",
                "baseline_total_playouts_per_repeat": int(baseline_total_playouts),
                "candidate_total_playouts_per_repeat": int(candidate_total_playouts),
                "total_paired_playouts_all_repeats": int(total_paired_playouts_all_repeats),
                "status": status,
                "description": args.description,
            },
        )

    print(f"Summary written to {summary_path}")
    print(f"Decision: {status} median_speedup_pct={corpus_median:.4f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
