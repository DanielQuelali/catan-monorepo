#!/usr/bin/env python3
import argparse
import difflib
import json
import subprocess
import sys
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate or verify the single-thread playout correctness corpus."
    )
    parser.add_argument("mode", choices=["generate", "verify"])
    parser.add_argument("--root", default=None, help="Repo root to evaluate")
    parser.add_argument(
        "--manifest",
        default=None,
        help="Path to correctness manifest JSON",
    )
    parser.add_argument("--suite", default=None, help="Manifest suite name")
    parser.add_argument(
        "--out-dir",
        default=None,
        help="Output directory for generated goldens or candidate artifacts",
    )
    parser.add_argument(
        "--golden-dir",
        default=None,
        help="Golden directory to compare against when mode=verify",
    )
    parser.add_argument(
        "--work-dir",
        default=None,
        help="Candidate artifact directory when mode=verify",
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


def suite_config(manifest: dict, suite_name: str | None) -> tuple[str, dict]:
    resolved = suite_name or manifest.get("default_suite")
    if not resolved:
        raise RuntimeError("Manifest is missing default_suite and no --suite was provided")
    suites = manifest.get("suites", {})
    if resolved not in suites:
        raise RuntimeError(f"Unknown suite {resolved}")
    return resolved, suites[resolved]


def write_run_config(path: Path, values: dict[str, str | int]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [f"{key}={value}" for key, value in values.items()]
    path.write_text("\n".join(lines) + "\n")


def generate_engine_report(root: Path, suite_name: str, suite: dict, out_dir: Path) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    write_run_config(
        out_dir / "run_config.txt",
        {
            "suite": suite_name,
            "seed_start": int(suite["seed_start"]),
            "seed_count": int(suite["seed_count"]),
            "max_turns": int(suite["max_turns"]),
        },
    )
    run(
        [
            "cargo",
            "run",
            "--release",
            "-p",
            "fastcore",
            "--bin",
            "deterministic_regression",
            "--",
            "--seed-start",
            str(suite["seed_start"]),
            "--seed-count",
            str(suite["seed_count"]),
            "--max-turns",
            str(suite["max_turns"]),
            "--out",
            str(out_dir / "engine_report.json"),
        ],
        root,
    )


def compare_text_files(expected: Path, actual: Path) -> None:
    expected_text = expected.read_text().splitlines(keepends=True)
    actual_text = actual.read_text().splitlines(keepends=True)
    if expected_text == actual_text:
        return
    diff = "".join(
        difflib.unified_diff(
            expected_text,
            actual_text,
            fromfile=str(expected),
            tofile=str(actual),
        )
    )
    raise RuntimeError(diff or f"Files differ: {expected} {actual}")


def load_json_file(path: Path) -> dict:
    return json.loads(path.read_text())


def unified_diff_text(expected: Path, actual: Path) -> str:
    expected_text = expected.read_text().splitlines(keepends=True)
    actual_text = actual.read_text().splitlines(keepends=True)
    return "".join(
        difflib.unified_diff(
            expected_text,
            actual_text,
            fromfile=str(expected),
            tofile=str(actual),
        )
    )


def first_seed_mismatch(expected: dict, actual: dict) -> tuple[int | None, dict | None, dict | None]:
    expected_rows = {int(row["seed"]): row for row in expected.get("per_seed", [])}
    actual_rows = {int(row["seed"]): row for row in actual.get("per_seed", [])}
    for seed in sorted(set(expected_rows) | set(actual_rows)):
        expected_row = expected_rows.get(seed)
        actual_row = actual_rows.get(seed)
        if expected_row != actual_row:
            return seed, expected_row, actual_row
    return None, None, None


def dump_candidate_failure_log(root: Path, suite: dict, seed: int, out_dir: Path) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    seeds_path = out_dir / "failing_seed.txt"
    seeds_path.write_text(f"{seed}\n")
    run(
        [
            "cargo",
            "run",
            "--release",
            "-p",
            "fastcore",
            "--bin",
            "deterministic_regression",
            "--",
            "--seeds-file",
            str(seeds_path),
            "--max-turns",
            str(suite["max_turns"]),
            "--dump-logs-dir",
            str(out_dir / "candidate_logs"),
            "--out",
            str(out_dir / "candidate_seed_report.json"),
        ],
        root,
    )


def compare_engine_reports(root: Path, suite: dict, golden_dir: Path, work_dir: Path) -> None:
    expected_path = golden_dir / "engine_report.json"
    actual_path = work_dir / "engine_report.json"
    if expected_path.read_text() == actual_path.read_text():
        return

    failure_dir = work_dir / "failure_dump"
    failure_dir.mkdir(parents=True, exist_ok=True)
    diff_text = unified_diff_text(expected_path, actual_path)
    (failure_dir / "engine_report.diff").write_text(diff_text)

    expected = load_json_file(expected_path)
    actual = load_json_file(actual_path)
    seed, expected_row, actual_row = first_seed_mismatch(expected, actual)
    summary = {
        "expected_report": str(expected_path),
        "actual_report": str(actual_path),
        "aggregate_expected": expected.get("aggregate"),
        "aggregate_actual": actual.get("aggregate"),
        "first_mismatching_seed": seed,
        "expected_seed_result": expected_row,
        "actual_seed_result": actual_row,
    }
    if seed is not None:
        dump_candidate_failure_log(root, suite, seed, failure_dir)
        summary["candidate_log_dir"] = str((failure_dir / "candidate_logs").resolve())
        summary["candidate_seed_report"] = str((failure_dir / "candidate_seed_report.json").resolve())
    (failure_dir / "mismatch_summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    raise RuntimeError(f"Engine report mismatch; failure dump written to {failure_dir}")


def main() -> int:
    args = parse_args()
    root = Path(args.root or Path(__file__).resolve().parents[2]).resolve()
    manifest_path = Path(args.manifest or root / "evals/single_thread/correctness_manifest.json")
    manifest = load_manifest(manifest_path)
    suite_name, suite = suite_config(manifest, args.suite)

    if args.mode == "generate":
        out_dir = Path(
            args.out_dir
            or root / "evals/artifacts/golden/correctness" / suite_name
        ).resolve()
        generate_engine_report(root, suite_name, suite, out_dir)
        print(f"Correctness goldens generated at {out_dir}")
        return 0

    golden_dir = Path(
        args.golden_dir
        or root / "evals/artifacts/golden/correctness" / suite_name
    ).resolve()
    work_dir = Path(
        args.work_dir
        or root / "evals/artifacts/verify/correctness" / suite_name
    ).resolve()
    if not golden_dir.is_dir():
        print(f"Golden directory not found: {golden_dir}", file=sys.stderr)
        return 2

    generate_engine_report(root, suite_name, suite, work_dir)
    compare_text_files(golden_dir / "run_config.txt", work_dir / "run_config.txt")
    compare_engine_reports(root, suite, golden_dir, work_dir)
    print(f"Correctness verification passed for suite {suite_name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
