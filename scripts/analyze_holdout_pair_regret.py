#!/usr/bin/env python3
"""Analyze highest-pip settlement-pair regret against optimal holdout picks.

Semantics:
- Group by unordered settlement pair.
- Evaluate each pair by its best branch (best order + roads).
- Branch win-rate uses weighted SIMS_RUN when available, else unweighted mean.
- If highest-pip ties exist on a board, include all tied pairs.
"""

from __future__ import annotations

import argparse
import csv
import json
import math
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

TILE_NODES: list[list[int]] = [
    [0, 1, 2, 3, 4, 5],
    [6, 7, 8, 9, 2, 1],
    [2, 9, 10, 11, 12, 3],
    [4, 3, 12, 13, 14, 15],
    [16, 5, 4, 15, 17, 18],
    [19, 20, 0, 5, 16, 21],
    [22, 23, 6, 1, 0, 20],
    [24, 25, 26, 27, 8, 7],
    [8, 27, 28, 29, 10, 9],
    [10, 29, 30, 31, 32, 11],
    [12, 11, 32, 33, 34, 13],
    [14, 13, 34, 35, 36, 37],
    [17, 15, 14, 37, 38, 39],
    [40, 18, 17, 39, 41, 42],
    [43, 21, 16, 18, 40, 44],
    [45, 46, 19, 21, 43, 47],
    [48, 49, 22, 20, 19, 46],
    [50, 51, 52, 23, 22, 49],
    [52, 53, 24, 7, 6, 23],
]

PORT_NODE_PAIRS: list[tuple[int, int]] = [
    (25, 26),
    (28, 29),
    (32, 33),
    (35, 36),
    (38, 39),
    (40, 44),
    (45, 47),
    (48, 49),
    (52, 53),
]

PIPS_BY_NUMBER: dict[int, int] = {
    2: 1,
    3: 2,
    4: 3,
    5: 4,
    6: 5,
    8: 5,
    9: 4,
    10: 3,
    11: 2,
    12: 1,
}

RESOURCE_ORDER = ["WOOD", "BRICK", "SHEEP", "WHEAT", "ORE"]
RESOURCE_ABBR = {
    "WOOD": "WO",
    "BRICK": "BR",
    "SHEEP": "SH",
    "WHEAT": "WH",
    "ORE": "OR",
}


def build_tiles_by_node() -> list[list[int]]:
    out: list[list[int]] = [[] for _ in range(54)]
    for tile_idx, nodes in enumerate(TILE_NODES):
        for node in nodes:
            out[node].append(tile_idx)
    for bucket in out:
        bucket.sort()
    return out


def build_adjacent_nodes() -> list[list[int]]:
    adjacent: list[set[int]] = [set() for _ in range(54)]
    for nodes in TILE_NODES:
        for i in range(6):
            a = nodes[i]
            b = nodes[(i + 1) % 6]
            adjacent[a].add(b)
            adjacent[b].add(a)
    return [sorted(list(neighbors)) for neighbors in adjacent]


TILES_BY_NODE = build_tiles_by_node()
ADJACENT_NODES = build_adjacent_nodes()


@dataclass
class NodePips:
    node: int
    total: int
    by_resource: dict[str, int]


@dataclass
class FollowerPlacement:
    color: str
    settlement: int
    road: tuple[int, int]


@dataclass
class FollowerVariantAgg:
    followers: list[FollowerPlacement]
    weighted_sims: float = 0.0
    unweighted_count: int = 0


@dataclass
class BranchAgg:
    settlement1: int
    road1: tuple[int, int]
    settlement2: int
    road2: tuple[int, int]
    weighted_win: float = 0.0
    weighted_sims: float = 0.0
    unweighted_win: float = 0.0
    unweighted_count: int = 0
    follower_variants: dict[str, FollowerVariantAgg] | None = None

    def add_row(self, win_white: float, sims_run: float | None) -> None:
        if sims_run is not None and sims_run > 0:
            self.weighted_win += win_white * sims_run
            self.weighted_sims += sims_run
            return
        self.unweighted_win += win_white
        self.unweighted_count += 1

    def win_rate(self) -> float:
        if self.weighted_sims > 0:
            return self.weighted_win / self.weighted_sims
        if self.unweighted_count > 0:
            return self.unweighted_win / self.unweighted_count
        return 0.0

    def add_follower_variant(
        self,
        variant_key: str,
        followers: list[FollowerPlacement],
        sims_run: float | None,
    ) -> None:
        if self.follower_variants is None:
            self.follower_variants = {}
        variant = self.follower_variants.get(variant_key)
        if variant is None:
            variant = FollowerVariantAgg(followers=followers)
            self.follower_variants[variant_key] = variant
        if sims_run is not None and sims_run > 0:
            variant.weighted_sims += sims_run
        else:
            variant.unweighted_count += 1

    def best_follower_variant(self) -> list[FollowerPlacement]:
        if not self.follower_variants:
            return []
        best_key = None
        best_variant = None
        for variant_key, variant in self.follower_variants.items():
            if best_variant is None:
                best_key = variant_key
                best_variant = variant
                continue
            if variant.weighted_sims > best_variant.weighted_sims:
                best_key = variant_key
                best_variant = variant
                continue
            if variant.weighted_sims < best_variant.weighted_sims:
                continue
            if variant.unweighted_count > best_variant.unweighted_count:
                best_key = variant_key
                best_variant = variant
                continue
            if variant.unweighted_count < best_variant.unweighted_count:
                continue
            if best_key is None or variant_key < best_key:
                best_key = variant_key
                best_variant = variant
        return best_variant.followers if best_variant else []


@dataclass
class Branch:
    settlement1: int
    road1: tuple[int, int]
    settlement2: int
    road2: tuple[int, int]
    win_rate: float
    followers: list[FollowerPlacement]


@dataclass
class PairGroup:
    settlement_a: int
    settlement_b: int
    best_branch: Branch
    pips_total: int
    pips_by_resource: dict[str, int]


def parse_road(token: str) -> tuple[int, int] | None:
    parts = str(token or "").split("-")
    if len(parts) != 2:
        return None
    try:
        a, b = int(parts[0]), int(parts[1])
    except ValueError:
        return None
    return (a, b) if a < b else (b, a)


def to_int(value: Any) -> int | None:
    try:
        return int(str(value))
    except (TypeError, ValueError):
        return None


def to_float(value: Any) -> float | None:
    try:
        return float(str(value))
    except (TypeError, ValueError):
        return None


def expand_numbers(tile_resources: list[Any], compact_numbers: list[Any]) -> list[int | None]:
    out: list[int | None] = [None] * len(tile_resources)
    ptr = 0
    for idx, resource in enumerate(tile_resources):
        if resource is None:
            continue
        out[idx] = int(compact_numbers[ptr]) if ptr < len(compact_numbers) else None
        ptr += 1
    return out


def tile_resource(board: dict[str, Any], tile_idx: int) -> str:
    resource = board["tile_resources"][tile_idx]
    return "DESERT" if resource is None else str(resource)


def pips_for_node(
    board: dict[str, Any],
    expanded_numbers: list[int | None],
    node: int,
) -> NodePips:
    by_resource = {resource: 0 for resource in RESOURCE_ORDER}
    total = 0
    for tile_idx in TILES_BY_NODE[node]:
        resource = tile_resource(board, tile_idx)
        if resource not in by_resource:
            continue
        number = expanded_numbers[tile_idx]
        pips = PIPS_BY_NUMBER.get(number, 0) if number is not None else 0
        by_resource[resource] += pips
        total += pips
    return NodePips(node=node, total=total, by_resource=by_resource)


def pips_for_pair(
    board: dict[str, Any],
    expanded_numbers: list[int | None],
    settlement_a: int,
    settlement_b: int,
) -> tuple[int, dict[str, int]]:
    by_resource = {resource: 0 for resource in RESOURCE_ORDER}
    total = 0
    for node in (settlement_a, settlement_b):
        node_pips = pips_for_node(board, expanded_numbers, node)
        for resource in RESOURCE_ORDER:
            by_resource[resource] += node_pips.by_resource[resource]
        total += node_pips.total
    return total, by_resource


def summarize_last_pick(
    board: dict[str, Any],
    expanded_numbers: list[int | None],
    branch: Branch,
) -> dict[str, Any]:
    settlement = branch.settlement2
    resource_counts = {resource: 0 for resource in RESOURCE_ORDER}
    for tile_idx in TILES_BY_NODE[settlement]:
        resource = tile_resource(board, tile_idx)
        if resource in resource_counts:
            resource_counts[resource] += 1
    return resource_counts


def last_pick_fields(prefix: str, counts: dict[str, int]) -> dict[str, int]:
    return {
        f"{prefix}_wood": counts["WOOD"],
        f"{prefix}_brick": counts["BRICK"],
        f"{prefix}_sheep": counts["SHEEP"],
        f"{prefix}_wheat": counts["WHEAT"],
        f"{prefix}_ore": counts["ORE"],
    }


def port_flags_for_pair(
    board: dict[str, Any],
    settlement_a: int,
    settlement_b: int,
) -> dict[str, int]:
    flags = {
        "3to1": 0,
        "wood": 0,
        "brick": 0,
        "sheep": 0,
        "wheat": 0,
        "ore": 0,
    }
    port_resources = board.get("port_resources", [])
    settlements = {settlement_a, settlement_b}
    for idx, node_pair in enumerate(PORT_NODE_PAIRS):
        if node_pair[0] not in settlements and node_pair[1] not in settlements:
            continue
        port = port_resources[idx] if idx < len(port_resources) else None
        if port is None:
            flags["3to1"] = 1
            continue
        label = str(port).strip().upper()
        if label == "WOOD":
            flags["wood"] = 1
        elif label == "BRICK":
            flags["brick"] = 1
        elif label == "SHEEP":
            flags["sheep"] = 1
        elif label == "WHEAT":
            flags["wheat"] = 1
        elif label == "ORE":
            flags["ore"] = 1
    return flags


def port_fields(prefix: str, flags: dict[str, int]) -> dict[str, int]:
    return {
        f"{prefix}_port_3to1": flags["3to1"],
        f"{prefix}_port_wood": flags["wood"],
        f"{prefix}_port_brick": flags["brick"],
        f"{prefix}_port_sheep": flags["sheep"],
        f"{prefix}_port_wheat": flags["wheat"],
        f"{prefix}_port_ore": flags["ore"],
    }


def parse_followers(row: dict[str, str]) -> list[FollowerPlacement]:
    followers: list[FollowerPlacement] = []
    for idx in range(1, 5):
        color = str(row.get(f"FOLLOWER{idx}_COLOR", "")).strip().upper()
        settlement = to_int(row.get(f"FOLLOWER{idx}_SETTLEMENT"))
        road = parse_road(str(row.get(f"FOLLOWER{idx}_ROAD", "")))
        if not color or settlement is None or road is None:
            continue
        followers.append(FollowerPlacement(color=color, settlement=settlement, road=road))
    return followers


def followers_key(followers: list[FollowerPlacement]) -> str:
    return "|".join(
        f"{f.color}:{f.settlement}:{f.road[0]}-{f.road[1]}" for f in followers
    )


def road_target_node(settlement: int, road: tuple[int, int]) -> int | None:
    a, b = road
    if settlement == a:
        return b
    if settlement == b:
        return a
    return None


def valid_road_expansion_candidates(
    settlement: int,
    road: tuple[int, int],
    occupied_settlements: list[int],
) -> list[int]:
    endpoint = road_target_node(settlement, road)
    if endpoint is None:
        return []

    blocked = set(occupied_settlements)
    for occupied in occupied_settlements:
        blocked.update(ADJACENT_NODES[occupied])

    candidates: list[int] = []
    for node in ADJACENT_NODES[endpoint]:
        if node == settlement:
            continue
        if node in blocked:
            continue
        candidates.append(node)
    return candidates


def road_target_candidates(
    board: dict[str, Any],
    expanded_numbers: list[int | None],
    branch: Branch,
) -> list[NodePips]:
    occupied_settlements = [
        branch.settlement1,
        branch.settlement2,
        *[follower.settlement for follower in branch.followers],
    ]
    candidates: set[int] = set()
    candidates.update(
        valid_road_expansion_candidates(
            settlement=branch.settlement1,
            road=branch.road1,
            occupied_settlements=occupied_settlements,
        )
    )
    candidates.update(
        valid_road_expansion_candidates(
            settlement=branch.settlement2,
            road=branch.road2,
            occupied_settlements=occupied_settlements,
        )
    )
    out = [pips_for_node(board, expanded_numbers, node) for node in sorted(candidates)]
    return out


def best_follower_pips(
    board: dict[str, Any],
    expanded_numbers: list[int | None],
    branch: Branch,
) -> tuple[str, NodePips] | None:
    if not branch.followers:
        return None
    best: tuple[str, NodePips] | None = None
    for follower in branch.followers:
        pip = pips_for_node(board, expanded_numbers, follower.settlement)
        if best is None:
            best = (follower.color, pip)
            continue
        _, best_pip = best
        if pip.total > best_pip.total:
            best = (follower.color, pip)
            continue
        if pip.total == best_pip.total and pip.node < best_pip.node:
            best = (follower.color, pip)
    return best


def build_pair_groups(rows: list[dict[str, str]], board: dict[str, Any]) -> list[PairGroup]:
    branches_by_key: dict[str, BranchAgg] = {}
    for row in rows:
        source = str(row.get("SOURCE", "")).strip().lower()
        if source and source != "holdout":
            continue

        s1 = to_int(row.get("LEADER_SETTLEMENT"))
        s2 = to_int(row.get("LEADER_SETTLEMENT2"))
        r1 = parse_road(str(row.get("LEADER_ROAD", "")))
        r2 = parse_road(str(row.get("LEADER_ROAD2", "")))
        win_white = to_float(row.get("WIN_WHITE"))
        sims_run = to_float(row.get("SIMS_RUN"))
        if s1 is None or s2 is None or r1 is None or r2 is None or win_white is None:
            continue

        key = f"{s1}|{r1[0]}-{r1[1]}|{s2}|{r2[0]}-{r2[1]}"
        agg = branches_by_key.get(key)
        if agg is None:
            agg = BranchAgg(settlement1=s1, road1=r1, settlement2=s2, road2=r2)
            branches_by_key[key] = agg
        agg.add_row(win_white, sims_run)

        followers = parse_followers(row)
        agg.add_follower_variant(
            variant_key=followers_key(followers),
            followers=followers,
            sims_run=sims_run,
        )

    branches = [
        Branch(
            settlement1=agg.settlement1,
            road1=agg.road1,
            settlement2=agg.settlement2,
            road2=agg.road2,
            win_rate=agg.win_rate(),
            followers=agg.best_follower_variant(),
        )
        for agg in branches_by_key.values()
    ]
    branches.sort(key=lambda b: (-b.win_rate, b.settlement1, b.road1, b.settlement2, b.road2))

    expanded_numbers = expand_numbers(board["tile_resources"], board["numbers"])
    groups: dict[str, PairGroup] = {}
    for branch in branches:
        a = min(branch.settlement1, branch.settlement2)
        b = max(branch.settlement1, branch.settlement2)
        pair_key = f"{a}+{b}"
        if pair_key in groups:
            continue
        total_pips, by_resource = pips_for_pair(board, expanded_numbers, a, b)
        groups[pair_key] = PairGroup(
            settlement_a=a,
            settlement_b=b,
            best_branch=branch,
            pips_total=total_pips,
            pips_by_resource=by_resource,
        )
    out = list(groups.values())
    out.sort(key=lambda g: (-g.best_branch.win_rate, g.settlement_a, g.settlement_b))
    return out


def split_board_list(value: str) -> list[str]:
    if re.fullmatch(r"\d{4}-\d{4}", value):
        start, end = value.split("-")
        return [f"{num:04d}" for num in range(int(start), int(end) + 1)]
    return [token.strip() for token in value.split(",") if token.strip()]


def format_res_comp(by_resource: dict[str, int]) -> str:
    return (
        f"WO:{by_resource['WOOD']} BR:{by_resource['BRICK']} SH:{by_resource['SHEEP']} "
        f"WH:{by_resource['WHEAT']} OR:{by_resource['ORE']}"
    )


def format_node_pips(value: NodePips | None) -> str:
    if value is None:
        return "-"
    return str(value.total)


def node_pips_json(value: NodePips | None) -> dict[str, Any] | None:
    if value is None:
        return None
    distinct, entropy = diversity_stats(value)
    return {
        "node": value.node,
        "total": value.total,
        "by_resource": value.by_resource,
        "diversity": {
            "distinct_resources": distinct,
            "entropy": round(entropy, 6),
        },
    }


def diversity_stats(value: NodePips) -> tuple[int, float]:
    total = value.total
    distinct = sum(1 for resource in RESOURCE_ORDER if value.by_resource[resource] > 0)
    if total <= 0:
        return distinct, 0.0
    entropy = 0.0
    for resource in RESOURCE_ORDER:
        amount = value.by_resource[resource]
        if amount <= 0:
            continue
        p = amount / total
        entropy -= p * math.log(p)
    return distinct, entropy


def choose_most_diverse_candidate(candidates: list[NodePips]) -> NodePips | None:
    if not candidates:
        return None

    def rank_key(value: NodePips) -> tuple[int, float, int, int]:
        distinct, entropy = diversity_stats(value)
        return (distinct, entropy, value.total, -value.node)

    best = candidates[0]
    best_key = rank_key(best)
    for candidate in candidates[1:]:
        candidate_key = rank_key(candidate)
        if candidate_key > best_key:
            best = candidate
            best_key = candidate_key
    return best


def node_pips_list_json(values: list[NodePips]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for value in values:
        encoded = node_pips_json(value)
        if encoded is not None:
            out.append(encoded)
    return out


def format_resource_counts(resource_counts: dict[str, int]) -> str:
    parts = []
    for resource in RESOURCE_ORDER:
        count = resource_counts.get(resource, 0)
        if count <= 0:
            continue
        parts.append(f"{RESOURCE_ABBR[resource]}x{count}")
    return " ".join(parts) if parts else "-"


def format_port_flags(prefix: str, row: dict[str, Any]) -> str:
    return (
        f"3:1={row[f'{prefix}_port_3to1']} "
        f"WO={row[f'{prefix}_port_wood']} "
        f"BR={row[f'{prefix}_port_brick']} "
        f"SH={row[f'{prefix}_port_sheep']} "
        f"WH={row[f'{prefix}_port_wheat']} "
        f"OR={row[f'{prefix}_port_ore']}"
    )


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Compute regret of highest-pip settlement pair(s) vs optimal pair per board."
    )
    parser.add_argument(
        "--boards",
        default="0001-0009",
        help="Board IDs as range (0001-0009) or comma list (0001,0002,0003).",
    )
    parser.add_argument(
        "--index",
        default="data/opening_states/index.json",
        help="Path to opening states index.json.",
    )
    parser.add_argument(
        "--states-root",
        default="data/opening_states",
        help="Root containing board files from index.json entries.",
    )
    parser.add_argument(
        "--analysis-root",
        default="data/analysis/opening_states",
        help="Root containing per-board holdout CSVs.",
    )
    parser.add_argument(
        "--format",
        choices=["table", "json"],
        default="table",
        help="Output format.",
    )
    args = parser.parse_args()

    index_path = Path(args.index)
    states_root = Path(args.states_root)
    analysis_root = Path(args.analysis_root)
    board_ids = split_board_list(args.boards)

    index_payload = json.loads(index_path.read_text(encoding="utf-8"))
    samples_by_id = {str(sample["id"]): sample for sample in index_payload["samples"]}

    results: list[dict[str, Any]] = []
    for board_id in board_ids:
        sample = samples_by_id.get(board_id)
        if sample is None:
            raise SystemExit(f"board {board_id} not found in {index_path}")

        board_path = states_root / sample["board_file"]
        csv_path = analysis_root / board_id / "initial_branch_analysis_all_sims_holdout.csv"
        if not board_path.exists():
            raise SystemExit(f"missing board file: {board_path}")
        if not csv_path.exists():
            raise SystemExit(f"missing holdout CSV: {csv_path}")

        board = json.loads(board_path.read_text(encoding="utf-8"))
        with csv_path.open("r", encoding="utf-8", newline="") as handle:
            rows = list(csv.DictReader(handle))

        groups = build_pair_groups(rows, board)
        if not groups:
            continue

        expanded_numbers = expand_numbers(board["tile_resources"], board["numbers"])
        optimal = groups[0]

        near_best_pairs = []
        all_pairs_best_branch = []
        for group in groups:
            gap_pp = optimal.best_branch.win_rate - group.best_branch.win_rate
            last_pick = summarize_last_pick(board, expanded_numbers, group.best_branch)
            pair_ports = port_flags_for_pair(board, group.settlement_a, group.settlement_b)
            pair_meta = {
                "pair": f"{group.settlement_a}+{group.settlement_b}",
                "win_pct": round(group.best_branch.win_rate, 3),
                "gap_pp": round(gap_pp, 3),
                "pips": group.pips_total,
                "resource_pips": group.pips_by_resource,
                "best_branch": {
                    "settlement1": group.best_branch.settlement1,
                    "road1": list(group.best_branch.road1),
                    "settlement2": group.best_branch.settlement2,
                    "road2": list(group.best_branch.road2),
                },
            }
            pair_meta.update(last_pick_fields("last_pick", last_pick))
            pair_meta.update(port_fields("pair", pair_ports))
            all_pairs_best_branch.append(pair_meta)
            if gap_pp <= 5.0 + 1e-9:
                near_best_pairs.append(pair_meta)

        max_pips = max(group.pips_total for group in groups)
        highest_pip_groups = [group for group in groups if group.pips_total == max_pips]
        highest_pip_groups.sort(key=lambda g: (g.settlement_a, g.settlement_b))

        for high in highest_pip_groups:
            regret_pp = optimal.best_branch.win_rate - high.best_branch.win_rate
            high_rt_candidates = road_target_candidates(board, expanded_numbers, high.best_branch)
            opt_rt_candidates = road_target_candidates(board, expanded_numbers, optimal.best_branch)
            high_rt = choose_most_diverse_candidate(high_rt_candidates)
            opt_rt = choose_most_diverse_candidate(opt_rt_candidates)
            high_follower = best_follower_pips(board, expanded_numbers, high.best_branch)
            opt_follower = best_follower_pips(board, expanded_numbers, optimal.best_branch)
            high_last_pick = summarize_last_pick(board, expanded_numbers, high.best_branch)
            opt_last_pick = summarize_last_pick(board, expanded_numbers, optimal.best_branch)
            high_ports = port_flags_for_pair(board, high.settlement_a, high.settlement_b)
            opt_ports = port_flags_for_pair(board, optimal.settlement_a, optimal.settlement_b)

            high_follower_json = None
            if high_follower is not None:
                high_color, high_pips = high_follower
                high_follower_json = {
                    "color": high_color,
                    "node": high_pips.node,
                    "total": high_pips.total,
                    "by_resource": high_pips.by_resource,
                }

            opt_follower_json = None
            if opt_follower is not None:
                opt_color, opt_pips = opt_follower
                opt_follower_json = {
                    "color": opt_color,
                    "node": opt_pips.node,
                    "total": opt_pips.total,
                    "by_resource": opt_pips.by_resource,
                }

            results.append(
                {
                    "board": board_id,
                    "high_pair": f"{high.settlement_a}+{high.settlement_b}",
                    "high_pips": high.pips_total,
                    "high_win_pct": round(high.best_branch.win_rate, 3),
                    "optimal_pair": f"{optimal.settlement_a}+{optimal.settlement_b}",
                    "optimal_pips": optimal.pips_total,
                    "optimal_win_pct": round(optimal.best_branch.win_rate, 3),
                    "pip_gap": high.pips_total - optimal.pips_total,
                    "regret_pp": round(regret_pp, 3),
                    "high_resource_pips": high.pips_by_resource,
                    "optimal_resource_pips": optimal.pips_by_resource,
                    "high_road_target": node_pips_json(high_rt),
                    "optimal_road_target": node_pips_json(opt_rt),
                    "high_road_target_candidates": node_pips_list_json(high_rt_candidates),
                    "optimal_road_target_candidates": node_pips_list_json(opt_rt_candidates),
                    "high_best_follower": high_follower_json,
                    "optimal_best_follower": opt_follower_json,
                    "pairs_within_5pp_of_best": near_best_pairs,
                    "all_pairs_best_branch": all_pairs_best_branch,
                    **last_pick_fields("high_last_pick", high_last_pick),
                    **last_pick_fields("optimal_last_pick", opt_last_pick),
                    **port_fields("high", high_ports),
                    **port_fields("optimal", opt_ports),
                }
            )

    if args.format == "json":
        print(json.dumps(results, indent=2))
        return

    print(
        "board  high_pair  high_pips  high_win  optimal_pair  optimal_pips  optimal_win  pip_gap  regret_pp  rt_hi  rt_opt  fol_hi  fol_opt"
    )
    print(
        "-----  ---------  ---------  --------  ------------  ------------  -----------  -------  ---------  -----  ------  ------  -------"
    )
    for row in results:
        high_rt_total = "-" if row["high_road_target"] is None else str(row["high_road_target"]["total"])
        opt_rt_total = "-" if row["optimal_road_target"] is None else str(row["optimal_road_target"]["total"])
        high_fol_total = "-" if row["high_best_follower"] is None else str(row["high_best_follower"]["total"])
        opt_fol_total = "-" if row["optimal_best_follower"] is None else str(row["optimal_best_follower"]["total"])

        print(
            f"{row['board']:>5}  {row['high_pair']:>9}  {row['high_pips']:>9}  "
            f"{row['high_win_pct']:>8.1f}  {row['optimal_pair']:>12}  {row['optimal_pips']:>12}  "
            f"{row['optimal_win_pct']:>11.1f}  {row['pip_gap']:>7}  {row['regret_pp']:>9.1f}  "
            f"{high_rt_total:>5}  {opt_rt_total:>6}  {high_fol_total:>6}  {opt_fol_total:>7}"
        )
        print(
            "       "
            f"high[{format_res_comp(row['high_resource_pips'])}]  "
            f"opt[{format_res_comp(row['optimal_resource_pips'])}]"
        )

        high_rt_res = "-" if row["high_road_target"] is None else format_res_comp(row["high_road_target"]["by_resource"])
        opt_rt_res = "-" if row["optimal_road_target"] is None else format_res_comp(row["optimal_road_target"]["by_resource"])
        print(f"       road-target high[{high_rt_res}]  opt[{opt_rt_res}]")

        high_fol_res = "-" if row["high_best_follower"] is None else format_res_comp(row["high_best_follower"]["by_resource"])
        opt_fol_res = "-" if row["optimal_best_follower"] is None else format_res_comp(row["optimal_best_follower"]["by_resource"])
        print(f"       follower     high[{high_fol_res}]  opt[{opt_fol_res}]")

        high_lp = {
            "WOOD": row["high_last_pick_wood"],
            "BRICK": row["high_last_pick_brick"],
            "SHEEP": row["high_last_pick_sheep"],
            "WHEAT": row["high_last_pick_wheat"],
            "ORE": row["high_last_pick_ore"],
        }
        opt_lp = {
            "WOOD": row["optimal_last_pick_wood"],
            "BRICK": row["optimal_last_pick_brick"],
            "SHEEP": row["optimal_last_pick_sheep"],
            "WHEAT": row["optimal_last_pick_wheat"],
            "ORE": row["optimal_last_pick_ore"],
        }
        print(
            "       "
            f"last-pick    high[{format_resource_counts(high_lp)}]  "
            f"opt[{format_resource_counts(opt_lp)}]"
        )
        print(
            "       "
            f"ports        high[{format_port_flags('high', row)}]  "
            f"opt[{format_port_flags('optimal', row)}]"
        )

    if results:
        avg_regret = sum(row["regret_pp"] for row in results) / len(results)
        max_regret = max(row["regret_pp"] for row in results)
        print("")
        print(f"rows={len(results)} avg_regret_pp={avg_regret:.2f} max_regret_pp={max_regret:.2f}")


if __name__ == "__main__":
    main()
