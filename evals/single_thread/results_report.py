#!/usr/bin/env python3
import argparse
import csv
import json
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Summarize autoresearch-style campaign results.tsv")
    parser.add_argument("--results-tsv", required=True, help="Path to campaign results.tsv")
    parser.add_argument("--json", action="store_true")
    return parser.parse_args()


def load_rows(path: Path) -> list[dict[str, str]]:
    if not path.exists():
        raise FileNotFoundError(f"Results TSV not found: {path}")
    with path.open("r", newline="") as handle:
        reader = csv.DictReader(handle, delimiter="\t")
        return list(reader)


def main() -> int:
    args = parse_args()
    results_path = Path(args.results_tsv).resolve()
    rows = load_rows(results_path)

    counts: dict[str, int] = {}
    best_keep = None
    kept = []
    for row in rows:
        status = row.get("status", "")
        counts[status] = counts.get(status, 0) + 1
        if status == "keep":
            kept.append(row)
            pps = float(row.get("playouts_per_cpu_second", "0") or 0.0)
            if best_keep is None or pps > float(best_keep.get("playouts_per_cpu_second", "0") or 0.0):
                best_keep = row

    payload = {
        "results_tsv": str(results_path),
        "total_experiments": len(rows),
        "status_counts": counts,
        "kept_count": len(kept),
        "best_keep": best_keep,
    }
    if args.json:
        print(json.dumps(payload, indent=2, sort_keys=True))
        return 0

    print(f"results_tsv={payload['results_tsv']}")
    print(f"total_experiments={payload['total_experiments']}")
    for status in sorted(counts):
        print(f"status_count_{status}={counts[status]}")
    print(f"kept_count={payload['kept_count']}")
    if best_keep:
        print("best_keep_commit=" + str(best_keep.get("commit", "")))
        print("best_keep_playouts_per_cpu_second=" + str(best_keep.get("playouts_per_cpu_second", "")))
        print("best_keep_median_speedup_pct=" + str(best_keep.get("median_speedup_pct", "")))
        print("best_keep_description=" + str(best_keep.get("description", "")))
    else:
        print("best_keep_commit=")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
