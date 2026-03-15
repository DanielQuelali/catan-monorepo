#!/usr/bin/env python3
import argparse
import json
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

TIME_RE = re.compile(r"^(real|user|sys) ([0-9]+(?:\.[0-9]+)?)$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run one immutable playout benchmark corpus pass.")
    parser.add_argument("--root", default=None, help="Repo root to benchmark")
    parser.add_argument("--manifest", default=None, help="Benchmark manifest path")
    parser.add_argument("--suite", default=None, help="Manifest suite name")
    parser.add_argument("--out", required=True, help="Output JSON path")
    parser.add_argument(
        "--warmup-runs",
        type=int,
        default=None,
        help="Warmup runs per workload (defaults to manifest value)",
    )
    parser.add_argument(
        "--skip-build",
        action="store_true",
        help="Skip release build step",
    )
    return parser.parse_args()


def run(cmd: list[str], cwd: Path) -> None:
    print("+", " ".join(cmd), file=sys.stderr)
    subprocess.run(cmd, cwd=cwd, check=True)


def load_manifest(path: Path) -> dict:
    data = json.loads(path.read_text())
    if data.get("schema_version") != 1:
        raise RuntimeError(f"Unsupported manifest schema_version in {path}")
    return data


def resolve_suite(manifest: dict, suite_name: str | None) -> tuple[str, dict]:
    resolved = suite_name or manifest.get("default_suite")
    if not resolved:
        raise RuntimeError("Manifest is missing default_suite and no --suite was provided")
    suites = manifest.get("suites", {})
    if resolved not in suites:
        raise RuntimeError(f"Unknown suite {resolved}")
    return resolved, suites[resolved]


def ensure_build(root: Path, workloads: list[dict]) -> None:
    kinds = {workload["kind"] for workload in workloads}
    unsupported = sorted(kind for kind in kinds if kind != "fastcore_bench_value_state")
    if unsupported:
        raise RuntimeError(f"Unsupported benchmark workload kind(s): {', '.join(unsupported)}")
    if "fastcore_bench_value_state" in kinds and not (root / "target/release/bench_value_state").exists():
        run(
            ["cargo", "build", "--release", "-p", "fastcore", "--bin", "bench_value_state"],
            root,
        )


def timed_run(cmd: list[str], cwd: Path) -> dict:
    full_cmd = ["/usr/bin/time", "-p", *cmd]
    completed = subprocess.run(
        full_cmd,
        cwd=cwd,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"Command failed ({completed.returncode}): {' '.join(cmd)}\n"
            f"stdout:\n{completed.stdout}\n"
            f"stderr:\n{completed.stderr}"
        )

    timing = {}
    stderr_lines = []
    for line in completed.stderr.splitlines():
        match = TIME_RE.match(line.strip())
        if match:
            timing[match.group(1)] = float(match.group(2))
        else:
            stderr_lines.append(line)

    if not {"real", "user", "sys"} <= timing.keys():
        raise RuntimeError(
            f"Failed to parse timing output for {' '.join(cmd)}\nstderr:\n{completed.stderr}"
        )

    return {
        "stdout": completed.stdout,
        "stderr": "\n".join(stderr_lines) + ("\n" if stderr_lines else ""),
        "wall_time_sec": timing["real"],
        "user_time_sec": timing["user"],
        "sys_time_sec": timing["sys"],
        "cpu_time_sec": timing["user"] + timing["sys"],
    }


def write_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def playouts_per_cpu_second(cpu_ns_per_playout: float) -> float:
    if cpu_ns_per_playout <= 0.0:
        return 0.0
    return 1_000_000_000.0 / cpu_ns_per_playout


def run_fastcore_workload(root: Path, workload: dict, out_dir: Path, warmups: int) -> dict:
    binary = root / "target/release/bench_value_state"
    if not binary.exists():
        raise RuntimeError(f"Missing binary {binary}; run without --skip-build first")

    args = [
        str(binary),
        "--state",
        str(root / workload["state"]),
        "--board",
        str(root / workload["board"]),
        "--sims",
        str(workload["sims"]),
        "--seed",
        str(workload["seed_start"]),
        "--max-turns",
        str(workload["max_turns"]),
    ]

    for warmup_idx in range(warmups):
        timed_run(args, root)
        print(f"Warmup {warmup_idx + 1}/{warmups} completed for {workload['id']}", file=sys.stderr)

    measured = timed_run(args, root)
    write_text(out_dir / "stdout.txt", measured["stdout"])
    write_text(out_dir / "stderr.txt", measured["stderr"])
    work_units = int(workload["sims"])
    cpu_ns = (measured["cpu_time_sec"] * 1_000_000_000.0) / work_units
    return {
        "id": workload["id"],
        "kind": workload["kind"],
        "critical": bool(workload.get("critical", False)),
        "tags": workload.get("tags", []),
        "command": args,
        "wall_time_sec": measured["wall_time_sec"],
        "user_time_sec": measured["user_time_sec"],
        "sys_time_sec": measured["sys_time_sec"],
        "cpu_time_sec": measured["cpu_time_sec"],
        "work_units": work_units,
        "work_unit_label": "playouts",
        "cpu_ns_per_work_unit": cpu_ns,
        "cpu_ns_per_playout": cpu_ns,
        "playouts_per_cpu_second": playouts_per_cpu_second(cpu_ns),
        "artifacts": {
            "stdout": str((out_dir / "stdout.txt").resolve()),
            "stderr": str((out_dir / "stderr.txt").resolve()),
        },
    }


def git_rev(root: Path) -> str:
    return (
        subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=root,
            capture_output=True,
            text=True,
            check=True,
        )
        .stdout.strip()
    )


def main() -> int:
    args = parse_args()
    root = Path(args.root or Path(__file__).resolve().parents[2]).resolve()
    manifest_path = Path(args.manifest or root / "evals/single_thread/benchmark_manifest.json")
    manifest = load_manifest(manifest_path)
    suite_name, suite = resolve_suite(manifest, args.suite)
    workloads = suite.get("workloads", [])
    warmups = args.warmup_runs if args.warmup_runs is not None else int(suite.get("warmup_runs", 0))
    out_path = Path(args.out).resolve()
    out_path.parent.mkdir(parents=True, exist_ok=True)
    run_dir = out_path.parent / out_path.stem
    run_dir.mkdir(parents=True, exist_ok=True)

    if not args.skip_build:
        ensure_build(root, workloads)

    results = []
    total_playouts = 0
    total_cpu_time_sec = 0.0
    total_wall_time_sec = 0.0
    for workload in workloads:
        workload_dir = run_dir / workload["id"]
        workload_dir.mkdir(parents=True, exist_ok=True)
        kind = workload["kind"]
        if kind != "fastcore_bench_value_state":
            raise RuntimeError(f"Unknown benchmark workload kind {kind}")
        result = run_fastcore_workload(root, workload, workload_dir, warmups)
        results.append(result)
        total_playouts += int(result["work_units"])
        total_cpu_time_sec += float(result["cpu_time_sec"])
        total_wall_time_sec += float(result["wall_time_sec"])
        print(
            f"Measured {workload['id']}: cpu_ns_per_playout={result['cpu_ns_per_playout']:.2f}",
            file=sys.stderr,
        )

    cpu_ns_per_playout = 0.0
    if total_playouts > 0 and total_cpu_time_sec > 0.0:
        cpu_ns_per_playout = (total_cpu_time_sec * 1_000_000_000.0) / total_playouts

    payload = {
        "format": "single-cpu-benchmark-run-v2",
        "suite": suite_name,
        "manifest": str(manifest_path.resolve()),
        "root": str(root),
        "commit": git_rev(root),
        "created_at": datetime.now(timezone.utc).isoformat(),
        "warmup_runs": warmups,
        "total_playouts": total_playouts,
        "total_cpu_time_sec": total_cpu_time_sec,
        "total_wall_time_sec": total_wall_time_sec,
        "cpu_ns_per_playout": cpu_ns_per_playout,
        "playouts_per_cpu_second": playouts_per_cpu_second(cpu_ns_per_playout),
        "workloads": results,
    }
    out_path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    print(f"Benchmark run written to {out_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
