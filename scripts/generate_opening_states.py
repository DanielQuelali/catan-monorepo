#!/usr/bin/env python3
"""Generate random Catan opening states with bot-picked red1/blue1/orange1."""

from __future__ import annotations

import argparse
import json
import random
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

TILE_COUNT = 19
OPENING_ACTION_COUNT = 6  # red1 S+R, blue1 S+R, orange1 S+R

# Standard Catan tile composition.
RESOURCE_BAG: list[str | None] = (
    ["WOOD"] * 4
    + ["BRICK"] * 3
    + ["SHEEP"] * 4
    + ["WHEAT"] * 4
    + ["ORE"] * 3
    + [None]
)

# Number tokens in official A->R order (desert skips one placement in traversal).
LETTER_NUMBER_TOKENS = [5, 2, 6, 3, 8, 10, 9, 12, 11, 4, 8, 10, 9, 4, 5, 6, 3, 11]

# Serpent traversal: start top-left, move clockwise around outer ring,
# then clockwise around inner ring, then center.
SERPENT_TILE_ORDER = [15, 16, 17, 18, 7, 8, 9, 10, 11, 12, 13, 14, 5, 6, 1, 2, 3, 4, 0]

# Four 3:1 ports plus one of each resource port.
PORT_BAG: list[str | None] = [None, None, None, None, "BRICK", "WOOD", "SHEEP", "WHEAT", "ORE"]

COLORS = ["RED", "BLUE", "ORANGE", "WHITE"]

SETTLEMENT_RE = re.compile(r"^(RED|BLUE|ORANGE|WHITE)\s+BUILD_SETTLEMENT\s+(\d+)$")
ROAD_RE = re.compile(r"^(RED|BLUE|ORANGE|WHITE)\s+BUILD_ROAD\s+\((\d+),\s*(\d+)\)$")
TURN_PREFIX_RE = re.compile(r"^(?:turn=\d+\s+)?(.*)$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Create randomized board layouts and opening state files using "
            "FastValueFunctionPlayer decisions for red1/blue1/orange1."
        )
    )
    parser.add_argument("--count", type=int, default=10, help="Number of samples to generate.")
    parser.add_argument("--seed", type=int, default=1, help="Master RNG seed.")
    parser.add_argument(
        "--out-dir",
        default="data/opening_states",
        help="Output directory for boards/states/index.",
    )
    parser.add_argument(
        "--log-binary",
        default=None,
        help="Optional path to prebuilt log_value_state binary.",
    )
    return parser.parse_args()


def ensure_log_binary(repo_root: Path, explicit_path: str | None) -> Path:
    if explicit_path:
        binary = Path(explicit_path).expanduser().resolve()
        if not binary.exists():
            raise FileNotFoundError(f"log binary not found: {binary}")
        return binary

    exe_name = "log_value_state.exe" if sys.platform.startswith("win") else "log_value_state"
    binary = repo_root / "target" / "debug" / exe_name
    if binary.exists():
        return binary

    subprocess.run(
        ["cargo", "build", "-q", "-p", "fastcore", "--bin", "log_value_state"],
        cwd=repo_root,
        check=True,
    )
    if not binary.exists():
        raise RuntimeError(f"expected built binary at {binary}")
    return binary


def generate_board_config(rng: random.Random) -> dict[str, Any]:
    resources = RESOURCE_BAG.copy()
    rng.shuffle(resources)
    desert_tile = resources.index(None)

    numbers_by_tile: list[int | None] = [None] * TILE_COUNT
    token_idx = 0
    for tile in SERPENT_TILE_ORDER:
        if tile == desert_tile:
            continue
        numbers_by_tile[tile] = LETTER_NUMBER_TOKENS[token_idx]
        token_idx += 1
    if token_idx != len(LETTER_NUMBER_TOKENS):
        raise RuntimeError("failed to place all number tokens")

    # board_from_json expects compact numbers list in tile-index order, skipping desert.
    numbers = [value for value in numbers_by_tile if value is not None]

    ports = PORT_BAG.copy()
    rng.shuffle(ports)

    return {
        "tile_resources": resources,
        "numbers": numbers,
        "port_resources": ports,
    }


def parse_action_log(stdout: str) -> list[list[Any]]:
    actions: list[list[Any]] = []
    for raw_line in stdout.splitlines():
        line = raw_line.strip()
        if not line:
            continue
        match = TURN_PREFIX_RE.match(line)
        if not match:
            continue
        line = match.group(1)

        settlement = SETTLEMENT_RE.match(line)
        if settlement:
            actions.append([settlement.group(1), "BUILD_SETTLEMENT", int(settlement.group(2))])
            continue

        road = ROAD_RE.match(line)
        if road:
            a = int(road.group(2))
            b = int(road.group(3))
            actions.append([road.group(1), "BUILD_ROAD", [a, b]])
            continue

        raise RuntimeError(f"unexpected action log line: {raw_line}")
    return actions


def run_opening_actions(log_binary: Path, board_path: Path, bot_seed: int) -> list[list[Any]]:
    minimal_state = {"colors": COLORS, "actions": []}
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as tmp:
        json.dump(minimal_state, tmp)
        tmp_path = Path(tmp.name)

    try:
        proc = subprocess.run(
            [str(log_binary), str(tmp_path), str(board_path), str(bot_seed), str(OPENING_ACTION_COUNT)],
            check=False,
            text=True,
            capture_output=True,
        )
    finally:
        tmp_path.unlink(missing_ok=True)

    if proc.returncode != 0:
        raise RuntimeError(
            "bot action generation failed.\n"
            f"command: {log_binary} <state> {board_path} {bot_seed} {OPENING_ACTION_COUNT}\n"
            f"exit_code: {proc.returncode}\n"
            f"stderr:\n{proc.stderr}"
        )

    actions = parse_action_log(proc.stdout)
    if len(actions) < OPENING_ACTION_COUNT:
        raise RuntimeError(
            f"expected at least {OPENING_ACTION_COUNT} opening actions, got {len(actions)}"
        )
    return actions[:OPENING_ACTION_COUNT]


def first_opening_placement(actions: list[list[Any]], color: str) -> dict[str, Any]:
    settlement: int | None = None
    road: list[int] | None = None
    for action in actions:
        if action[0] != color:
            continue
        if action[1] == "BUILD_SETTLEMENT" and settlement is None:
            settlement = int(action[2])
        elif action[1] == "BUILD_ROAD" and road is None:
            road = [int(action[2][0]), int(action[2][1])]
        if settlement is not None and road is not None:
            break
    if settlement is None or road is None:
        raise RuntimeError(f"missing opening settlement/road for {color}")
    return {"settlement": settlement, "road": road}


def build_state(actions: list[list[Any]], placements: dict[str, Any], board_file: str) -> dict[str, Any]:
    return {
        "colors": COLORS,
        "bot_colors": COLORS,
        "is_initial_build_phase": True,
        "current_color": "WHITE",
        "current_prompt": "BUILD_INITIAL_SETTLEMENT",
        "state_index": len(actions),
        "board_file": board_file,
        "actions": actions,
        "placements": placements,
    }


def main() -> int:
    args = parse_args()
    if args.count <= 0:
        raise ValueError("--count must be > 0")

    repo_root = Path(__file__).resolve().parents[1]
    out_dir = Path(args.out_dir).expanduser()
    boards_dir = out_dir / "boards"
    states_dir = out_dir / "states"
    boards_dir.mkdir(parents=True, exist_ok=True)
    states_dir.mkdir(parents=True, exist_ok=True)

    log_binary = ensure_log_binary(repo_root, args.log_binary)
    master_rng = random.Random(args.seed)

    records: list[dict[str, Any]] = []
    for idx in range(1, args.count + 1):
        board_seed = master_rng.getrandbits(64)
        bot_seed = master_rng.getrandbits(64)

        board_rng = random.Random(board_seed)
        board = generate_board_config(board_rng)

        board_name = f"board_{idx:04d}.json"
        state_name = f"state_{idx:04d}.json"
        board_path = boards_dir / board_name
        state_path = states_dir / state_name
        board_path.write_text(json.dumps(board, indent=2) + "\n", encoding="utf-8")

        actions = run_opening_actions(log_binary, board_path, bot_seed)
        placements = {
            "red1": first_opening_placement(actions, "RED"),
            "blue1": first_opening_placement(actions, "BLUE"),
            "orange1": first_opening_placement(actions, "ORANGE"),
        }
        state = build_state(actions, placements, f"boards/{board_name}")
        state_path.write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")

        records.append(
            {
                "id": f"{idx:04d}",
                "board_file": f"boards/{board_name}",
                "state_file": f"states/{state_name}",
                "board_seed": board_seed,
                "bot_seed": bot_seed,
                "red1": placements["red1"],
                "blue1": placements["blue1"],
                "orange1": placements["orange1"],
            }
        )

    index = {
        "schema_version": 1,
        "generator": "scripts/generate_opening_states.py",
        "count": args.count,
        "seed": args.seed,
        "samples": records,
    }
    index_path = out_dir / "index.json"
    index_path.write_text(json.dumps(index, indent=2) + "\n", encoding="utf-8")

    print(f"Generated {args.count} opening states in {out_dir}")
    print(f"Index: {index_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
