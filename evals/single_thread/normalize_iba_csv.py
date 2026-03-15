#!/usr/bin/env python3
import csv
import sys
from pathlib import Path

NONDETERMINISTIC_COLUMNS = {
    "TIMESTAMP_UTC",
    "HOST_CPU_COUNT",
    "WORKERS_USED",
    "WALL_TIME_SEC",
    "CPU_TIME_SEC",
}


def main() -> int:
    if len(sys.argv) != 3:
        print("Usage: normalize_iba_csv.py <input.csv> <output.csv>", file=sys.stderr)
        return 2

    src = Path(sys.argv[1])
    dst = Path(sys.argv[2])
    with src.open("r", newline="") as in_f:
        reader = csv.DictReader(in_f)
        fieldnames = reader.fieldnames
        if fieldnames is None:
            raise RuntimeError(f"{src} has no CSV header")

        rows = []
        for row in reader:
            normalized = dict(row)
            for key in NONDETERMINISTIC_COLUMNS:
                if key in normalized:
                    normalized[key] = "0"
            rows.append(normalized)

    dst.parent.mkdir(parents=True, exist_ok=True)
    with dst.open("w", newline="") as out_f:
        writer = csv.DictWriter(out_f, fieldnames=fieldnames, lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
