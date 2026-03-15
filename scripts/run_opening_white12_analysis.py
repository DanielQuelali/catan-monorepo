#!/usr/bin/env python3
"""Run initial branch analysis over opening-state samples."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Run initial-branch-analysis for each sample in an opening "
            "states index and emit per-sample CSV outputs."
        )
    )
    parser.add_argument(
        "--index",
        default="data/opening_states/index.json",
        help="Path to opening states index.json",
    )
    parser.add_argument(
        "--out-root",
        default="data/analysis/opening_states",
        help="Root output directory for per-sample analysis artifacts",
    )
    parser.add_argument(
        "--budget",
        type=int,
        default=1000,
        help="Stackelberg budget passed to initial-branch-analysis",
    )
    parser.add_argument(
        "--num-sims",
        type=int,
        default=0,
        help="Holdout simulation count passed to initial-branch-analysis",
    )
    parser.add_argument(
        "--workers",
        type=int,
        default=None,
        help="Optional worker count passed to initial-branch-analysis",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=99,
        help="Base seed passed to initial-branch-analysis",
    )
    parser.add_argument(
        "--start-seed",
        type=int,
        default=1000,
        help="Start seed passed to initial-branch-analysis",
    )
    parser.add_argument(
        "--max-turns",
        type=int,
        default=1000,
        help="Max turns passed to initial-branch-analysis",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=None,
        help="Optional branch-task limit passed to initial-branch-analysis",
    )
    parser.add_argument(
        "--sample-limit",
        type=int,
        default=None,
        help="Optional number of samples to run from the index",
    )
    parser.add_argument(
        "--exclude-sample-ids",
        default="",
        help="Comma-separated sample IDs to skip (e.g. 0001,0003)",
    )
    parser.add_argument(
        "--holdout-rerun",
        action="store_true",
        help="Use legacy holdout rerun behavior instead of TS-reuse holdout",
    )
    parser.add_argument(
        "--holdout-only",
        action="store_true",
        help="Emit only holdout all-sims CSV output (skip TS output/materialization)",
    )
    parser.add_argument(
        "--no-endgame-gap",
        action="store_true",
        help="Disable endgame-gap heuristic term (enabled by default).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help=(
            "Pass --dry-run to initial-branch-analysis to generate CSV outputs "
            "without executing simulations"
        ),
    )
    parser.add_argument(
        "--cargo-bin",
        default="cargo",
        help="Cargo executable to invoke when building release binary",
    )
    parser.add_argument(
        "--analysis-bin",
        default=None,
        help=(
            "Optional path to an initial-branch-analysis binary. "
            "If omitted, this script builds and uses target/release/initial-branch-analysis."
        ),
    )
    return parser.parse_args()


def load_samples(index_path: Path) -> list[dict]:
    with index_path.open("r", encoding="utf-8") as f:
        payload = json.load(f)
    samples = payload.get("samples")
    if not isinstance(samples, list) or not samples:
        raise ValueError(f"index has no samples: {index_path}")
    return samples


def sample_id(sample: dict, idx: int) -> str:
    raw = sample.get("id")
    if isinstance(raw, str) and raw:
        return raw
    return f"{idx + 1:04d}"


def run_sample(
    args: argparse.Namespace,
    analysis_bin: Path,
    index_dir: Path,
    sample: dict,
    idx: int,
    out_root: Path,
) -> None:
    sid = sample_id(sample, idx)

    board_file = sample.get("board_file")
    state_file = sample.get("state_file")
    if not isinstance(board_file, str) or not board_file:
        raise ValueError(f"sample {sid} missing board_file")
    if not isinstance(state_file, str) or not state_file:
        raise ValueError(f"sample {sid} missing state_file")

    board_path = (index_dir / board_file).resolve()
    state_path = (index_dir / state_file).resolve()
    if not board_path.exists():
        raise FileNotFoundError(f"board file not found for sample {sid}: {board_path}")
    if not state_path.exists():
        raise FileNotFoundError(f"state file not found for sample {sid}: {state_path}")

    sample_out = out_root / sid
    sample_out.mkdir(parents=True, exist_ok=True)
    output_csv = sample_out / "initial_branch_analysis.csv"
    all_sims_csv = sample_out / "initial_branch_analysis_all_sims.csv"

    cmd = [
        str(analysis_bin),
        "--state",
        str(state_path),
        "--board",
        str(board_path),
        "--output",
        str(output_csv),
        "--all-sims-output",
        str(all_sims_csv),
        "--num-sims",
        str(args.num_sims),
        "--budget",
        str(args.budget),
        "--seed",
        str(args.seed),
        "--start-seed",
        str(args.start_seed),
        "--max-turns",
        str(args.max_turns),
    ]
    if args.dry_run:
        cmd.append("--dry-run")
    if args.workers is not None:
        cmd.extend(["--workers", str(args.workers)])
    if args.holdout_rerun:
        cmd.append("--holdout-rerun")
    if args.holdout_only:
        cmd.append("--holdout-only")
    if args.no_endgame_gap:
        cmd.append("--no-endgame-gap")
    if args.limit is not None:
        cmd.extend(["--limit", str(args.limit)])

    print(
        (
            f"[{idx + 1}] sample={sid} state={state_file} board={board_file}"
            f"{' dry_run=true' if args.dry_run else ''}"
        ),
        flush=True,
    )
    subprocess.run(cmd, check=True)


def resolve_analysis_bin(args: argparse.Namespace, repo_root: Path) -> Path:
    if args.analysis_bin is not None:
        analysis_bin = Path(args.analysis_bin).expanduser().resolve()
        if not analysis_bin.exists():
            raise FileNotFoundError(f"analysis bin not found: {analysis_bin}")
        return analysis_bin

    subprocess.run(
        [args.cargo_bin, "build", "--release", "-p", "initial-branch-analysis"],
        check=True,
        cwd=repo_root,
    )
    analysis_bin = (repo_root / "target" / "release" / "initial-branch-analysis").resolve()
    if not analysis_bin.exists():
        raise FileNotFoundError(f"release analysis bin not found after build: {analysis_bin}")
    return analysis_bin


def main() -> int:
    args = parse_args()
    index_path = Path(args.index).expanduser().resolve()
    if not index_path.exists():
        raise FileNotFoundError(f"index not found: {index_path}")

    out_root = Path(args.out_root).expanduser().resolve()
    out_root.mkdir(parents=True, exist_ok=True)

    repo_root = Path(__file__).resolve().parent.parent
    analysis_bin = resolve_analysis_bin(args, repo_root)

    index_dir = index_path.parent
    samples = load_samples(index_path)
    exclude_sample_ids = {
        sid.strip() for sid in args.exclude_sample_ids.split(",") if sid.strip()
    }
    if exclude_sample_ids:
        samples = [
            sample
            for idx, sample in enumerate(samples)
            if sample_id(sample, idx) not in exclude_sample_ids
        ]

    if args.sample_limit is not None:
        samples = samples[: max(0, args.sample_limit)]

    print(f"Loaded {len(samples)} samples from {index_path}", flush=True)
    for idx, sample in enumerate(samples):
        run_sample(args, analysis_bin, index_dir, sample, idx, out_root)

    print(f"Completed {len(samples)} samples. Outputs under {out_root}", flush=True)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as exc:
        print(f"Command failed with exit code {exc.returncode}", file=sys.stderr)
        raise
