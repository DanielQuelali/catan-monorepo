const TILE_COORDS = [
  [0, 0, 0], [1, -1, 0], [0, -1, 1], [-1, 0, 1], [-1, 1, 0], [0, 1, -1], [1, 0, -1],
  [2, -2, 0], [1, -2, 1], [0, -2, 2], [-1, -1, 2], [-2, 0, 2], [-2, 1, 1], [-2, 2, 0],
  [-1, 2, -1], [0, 2, -2], [1, 1, -2], [2, 0, -2], [2, -1, -1],
];

const TILE_NODES = [
  [0, 1, 2, 3, 4, 5], [6, 7, 8, 9, 2, 1], [2, 9, 10, 11, 12, 3], [4, 3, 12, 13, 14, 15],
  [16, 5, 4, 15, 17, 18], [19, 20, 0, 5, 16, 21], [22, 23, 6, 1, 0, 20],
  [24, 25, 26, 27, 8, 7], [8, 27, 28, 29, 10, 9], [10, 29, 30, 31, 32, 11],
  [12, 11, 32, 33, 34, 13], [14, 13, 34, 35, 36, 37], [17, 15, 14, 37, 38, 39],
  [40, 18, 17, 39, 41, 42], [43, 21, 16, 18, 40, 44], [45, 46, 19, 21, 43, 47],
  [48, 49, 22, 20, 19, 46], [50, 51, 52, 23, 22, 49], [52, 53, 24, 7, 6, 23],
];

const PORT_NODE_PAIRS = [
  [25, 26], [28, 29], [32, 33], [35, 36], [38, 39], [40, 44], [45, 47], [48, 49], [52, 53],
];

const SERPENT_TILE_ORDER = [15, 16, 17, 18, 7, 8, 9, 10, 11, 12, 13, 14, 5, 6, 1, 2, 3, 4, 0];
const TOKEN_LETTERS = "ABCDEFGHIJKLMNOPQR".split("");
const TILE_INDICES_BY_NODE = buildTileIndicesByNode();

const RESOURCE_FILL = {
  WOOD: "#4f9d69",
  BRICK: "#b75d44",
  SHEEP: "#9ac26a",
  WHEAT: "#d8bc59",
  ORE: "#7a7f8a",
  DESERT: "#d5bd8a",
};

const RESOURCE_PIP_LABELS = [
  { resource: "WOOD", label: "WO" },
  { resource: "BRICK", label: "BR" },
  { resource: "SHEEP", label: "SH" },
  { resource: "WHEAT", label: "WH" },
  { resource: "ORE", label: "OR" },
];

const PIPS_BY_NUMBER = {
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
};

const PLAYER_STROKE = {
  RED: "#c62828",
  BLUE: "#1565c0",
  ORANGE: "#ef6c00",
  WHITE: "#9aa1ad",
};

const POLL_MS = 1200;
const HOLDOUT_CSV_BASENAME = "initial_branch_analysis_all_sims_holdout.csv";
const BOARD_VIEWBOX_PADDING = 16;
const ROAD_DIRECTION_VECTORS = [
  { name: "north", x: 0, y: -1 },
  { name: "northeast", x: Math.sqrt(3) / 2, y: -0.5 },
  { name: "southeast", x: Math.sqrt(3) / 2, y: 0.5 },
  { name: "south", x: 0, y: 1 },
  { name: "southwest", x: -Math.sqrt(3) / 2, y: 0.5 },
  { name: "northwest", x: -Math.sqrt(3) / 2, y: -0.5 },
];

const svg = document.getElementById("board");
const prevBtn = document.getElementById("prevBtn");
const nextBtn = document.getElementById("nextBtn");
const sampleSelect = document.getElementById("sampleSelect");
const statusEl = document.getElementById("status");
const dataPathEl = document.getElementById("dataPath");
const metaText = document.getElementById("metaText");
const portsEl = document.getElementById("ports");
const placementsText = document.getElementById("placementsText");
const holdoutList = document.getElementById("holdoutList");
const wrapEl = document.querySelector(".wrap");

const HOLDOUT_WIN_STAT_METRICS = [
  { suffix: "PCT_HAS_SETTLEMENT", label: "Has settlement", format: "percent", decimals: 1 },
  { suffix: "AVG_SETTLEMENTS", label: "Avg. Settlements", format: "number", decimals: 2 },
  { suffix: "AVG_CITIES", label: "Avg. Cities", format: "number", decimals: 2 },
  { suffix: "PCT_HAS_CITY", label: "Has city", format: "percent", decimals: 1 },
  { suffix: "AVG_TURN_FIRST_SETTLEMENT", label: "First settlement turn", format: "number", decimals: 2 },
  { suffix: "AVG_TURN_FIRST_CITY", label: "First city turn", format: "number", decimals: 2 },
  { suffix: "PCT_HAS_VP", label: "Has VP", format: "percent", decimals: 1 },
  { suffix: "AVG_VP_GIVEN_HAS", label: "Avg VP | has", format: "number", decimals: 2 },
  { suffix: "PCT_LA", label: "Largest Army", format: "percent", decimals: 1 },
  { suffix: "PCT_LR", label: "Longest Road", format: "percent", decimals: 1 },
  { suffix: "PCT_BOTH", label: "Both LA+LR", format: "percent", decimals: 1 },
  { suffix: "PCT_PLAYED_MONOPOLY", label: "Played Monopoly", format: "percent", decimals: 1 },
  { suffix: "PCT_PLAYED_YOP", label: "Played YOP", format: "percent", decimals: 1 },
  { suffix: "PCT_PLAYED_ROAD_BUILDER", label: "Played Road Builder", format: "percent", decimals: 1 },
  { suffix: "PCT_PLAYED_KNIGHTS", label: "Played Knights", format: "percent", decimals: 1 },
  { suffix: "AVG_KNIGHTS_GIVEN_PLAYED", label: "Avg Knights | played", format: "number", decimals: 2 },
];

const HOLDOUT_WIN_STAT_BY_SUFFIX = new Map(
  HOLDOUT_WIN_STAT_METRICS.map((metric) => [metric.suffix, metric])
);

const HOLDOUT_WIN_STAT_DISPLAY_SUFFIXES = [
  "PCT_LA",
  "PCT_LR",
  "PCT_HAS_VP",
  "AVG_SETTLEMENTS",
  "AVG_CITIES",
];

const params = new URLSearchParams(window.location.search);
const dataDirParam = params.get("data");
const analysisDirParam = params.get("analysis");
const DATA_ROOT = new URL(dataDirParam || "/data/opening_states/", window.location.href);
const ANALYSIS_ROOT = new URL(analysisDirParam || "/runtime-data/opening_states/", window.location.href);

let samples = [];
let current = 0;
let indexPayload = null;
let indexTextCache = null;
let boardTextCache = null;
let stateTextCache = null;
let pollTimer = null;
let loadInProgress = false;
const holdoutCache = new Map();
let activeBoard = null;
let activePlacements = null;
let activeHoverPick = null;
let activeHoverPickIndex = null;
let activeHoverBranchKey = null;

function buildTileIndicesByNode() {
  const out = [];
  for (let tileIndex = 0; tileIndex < TILE_NODES.length; tileIndex += 1) {
    const nodes = TILE_NODES[tileIndex];
    for (const nodeId of nodes) {
      if (!out[nodeId]) out[nodeId] = [];
      out[nodeId].push(tileIndex);
    }
  }
  for (const bucket of out) {
    if (!bucket) continue;
    bucket.sort((a, b) => a - b);
  }
  return out;
}

function setStatus(text) {
  statusEl.textContent = text;
}

function withCacheBust(url) {
  const u = new URL(url);
  u.searchParams.set("_", String(Date.now()));
  return u.toString();
}

function dataUrl(relPath) {
  return new URL(relPath, DATA_ROOT).toString();
}

function analysisUrl(relPath) {
  return new URL(relPath, ANALYSIS_ROOT).toString();
}

async function fetchText(relPath) {
  const url = withCacheBust(dataUrl(relPath));
  const res = await fetch(url, { cache: "no-store" });
  if (!res.ok) {
    throw new Error(`Failed to load ${relPath} (${res.status})`);
  }
  return res.text();
}

async function fetchAnalysisText(relPath) {
  const url = withCacheBust(analysisUrl(relPath));
  const res = await fetch(url, { cache: "no-store" });
  if (!res.ok) {
    throw new Error(`Failed to load ${relPath} (${res.status})`);
  }
  return res.text();
}

function parseJson(text, relPath) {
  try {
    return JSON.parse(text);
  } catch (error) {
    throw new Error(`Invalid JSON in ${relPath}: ${error.message}`);
  }
}

function splitCsvLine(line) {
  const out = [];
  let cur = "";
  let inQuotes = false;

  for (let i = 0; i < line.length; i += 1) {
    const ch = line[i];
    if (ch === '"') {
      if (inQuotes && line[i + 1] === '"') {
        cur += '"';
        i += 1;
      } else {
        inQuotes = !inQuotes;
      }
      continue;
    }
    if (ch === "," && !inQuotes) {
      out.push(cur);
      cur = "";
      continue;
    }
    cur += ch;
  }
  out.push(cur);
  return out;
}

function parseCsv(text) {
  const lines = text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  if (lines.length < 2) return [];

  const headers = splitCsvLine(lines[0]);
  return lines.slice(1).map((line) => {
    const cells = splitCsvLine(line);
    const row = {};
    for (let idx = 0; idx < headers.length; idx += 1) {
      row[headers[idx]] = cells[idx] ?? "";
    }
    return row;
  });
}

function toInt(value) {
  const parsed = Number(value);
  return Number.isInteger(parsed) ? parsed : null;
}

function toNumber(value) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function parseLeaderWinStatsFromRow(row) {
  const leaderColor = String(row.LEADER_COLOR || "").trim().toUpperCase();
  if (!leaderColor) return null;

  const values = {};
  for (const metric of HOLDOUT_WIN_STAT_METRICS) {
    const key = `WIN_${leaderColor}_${metric.suffix}`;
    const value = toNumber(row[key]);
    if (value === null) return null;
    values[metric.suffix] = value;
  }

  return { leaderColor, values };
}

function parseRoadToken(token) {
  const parts = String(token || "")
    .split("-")
    .map((value) => Number(value));
  if (parts.length !== 2 || !Number.isFinite(parts[0]) || !Number.isFinite(parts[1])) {
    return null;
  }
  const a = Math.min(parts[0], parts[1]);
  const b = Math.max(parts[0], parts[1]);
  return [a, b];
}

function compareRoad(a, b) {
  if (a[0] !== b[0]) return a[0] - b[0];
  return a[1] - b[1];
}

function compareBranchPlacement(a, b) {
  if (b.winWhite !== a.winWhite) return b.winWhite - a.winWhite;
  if (a.settlement1 !== b.settlement1) return a.settlement1 - b.settlement1;
  const road1Cmp = compareRoad(a.road1, b.road1);
  if (road1Cmp !== 0) return road1Cmp;
  if (a.settlement2 !== b.settlement2) return a.settlement2 - b.settlement2;
  return compareRoad(a.road2, b.road2);
}

function parseFollowersFromRow(row) {
  const followers = [];
  for (let idx = 1; idx <= 4; idx += 1) {
    const color = String(row[`FOLLOWER${idx}_COLOR`] || "").trim().toUpperCase();
    const settlement = toInt(row[`FOLLOWER${idx}_SETTLEMENT`]);
    const road = parseRoadToken(row[`FOLLOWER${idx}_ROAD`]);
    if (!color || settlement === null || road === null || !PLAYER_STROKE[color]) continue;
    followers.push({ color, settlement, road });
  }
  return followers;
}

function followersKey(followers) {
  return followers
    .map((f) => `${f.color}:${f.settlement}:${f.road[0]}-${f.road[1]}`)
    .join("|");
}

function bestFollowerVariant(variants) {
  let best = null;
  for (const variant of variants.values()) {
    if (!best) {
      best = variant;
      continue;
    }
    if (variant.weightedSims > best.weightedSims) {
      best = variant;
      continue;
    }
    if (variant.weightedSims < best.weightedSims) continue;
    if (variant.unweightedCount > best.unweightedCount) {
      best = variant;
      continue;
    }
    if (variant.unweightedCount < best.unweightedCount) continue;
    if (followersKey(variant.followers) < followersKey(best.followers)) {
      best = variant;
    }
  }
  return best ? best.followers : [];
}

function buildHoldoutTopPicks(rows) {
  const aggregates = new Map();
  let hasEnhancedRows = false;

  for (const row of rows) {
    const source = String(row.SOURCE || "").trim().toLowerCase();
    if (source && source !== "holdout") continue;

    const s1 = toInt(row.LEADER_SETTLEMENT);
    const r1 = parseRoadToken(row.LEADER_ROAD);
    const s2 = toInt(row.LEADER_SETTLEMENT2);
    const r2 = parseRoadToken(row.LEADER_ROAD2);
    const winWhite = toNumber(row.WIN_WHITE);
    const simsRun = toNumber(row.SIMS_RUN);
    const followers = parseFollowersFromRow(row);
    if (s1 === null || r1 === null || s2 === null || r2 === null || winWhite === null) continue;
    const leaderWinStats = parseLeaderWinStatsFromRow(row);
    if (leaderWinStats) {
      hasEnhancedRows = true;
    }

    const key = `${s1}|${r1[0]}-${r1[1]}|${s2}|${r2[0]}-${r2[1]}`;
    let agg = aggregates.get(key);
    if (!agg) {
      agg = {
        s1,
        r1,
        s2,
        r2,
        weightedWin: 0,
        weightedSims: 0,
        unweightedWin: 0,
        unweightedCount: 0,
        followerVariants: new Map(),
        winStatsLeaderColor: null,
        winStatsWeightedSims: 0,
        winStatsUnweightedCount: 0,
        winStatsWeightedSums: Object.fromEntries(HOLDOUT_WIN_STAT_METRICS.map((metric) => [metric.suffix, 0])),
        winStatsUnweightedSums: Object.fromEntries(HOLDOUT_WIN_STAT_METRICS.map((metric) => [metric.suffix, 0])),
      };
      aggregates.set(key, agg);
    }

    if (simsRun !== null && simsRun > 0) {
      agg.weightedWin += winWhite * simsRun;
      agg.weightedSims += simsRun;
    } else {
      agg.unweightedWin += winWhite;
      agg.unweightedCount += 1;
    }

    const fKey = followersKey(followers);
    const variant = agg.followerVariants.get(fKey) || {
      followers,
      weightedSims: 0,
      unweightedCount: 0,
    };
    if (simsRun !== null && simsRun > 0) {
      variant.weightedSims += simsRun;
    } else {
      variant.unweightedCount += 1;
    }
    agg.followerVariants.set(fKey, variant);

    if (
      leaderWinStats
      && (agg.winStatsLeaderColor === null || agg.winStatsLeaderColor === leaderWinStats.leaderColor)
    ) {
      agg.winStatsLeaderColor = leaderWinStats.leaderColor;
      if (simsRun !== null && simsRun > 0) {
        agg.winStatsWeightedSims += simsRun;
        for (const metric of HOLDOUT_WIN_STAT_METRICS) {
          agg.winStatsWeightedSums[metric.suffix] += leaderWinStats.values[metric.suffix] * simsRun;
        }
      } else {
        agg.winStatsUnweightedCount += 1;
        for (const metric of HOLDOUT_WIN_STAT_METRICS) {
          agg.winStatsUnweightedSums[metric.suffix] += leaderWinStats.values[metric.suffix];
        }
      }
    }
  }

  const branches = [];
  for (const agg of aggregates.values()) {
    let winWhite = 0;
    let sampleSize = 0;
    if (agg.weightedSims > 0) {
      winWhite = agg.weightedWin / agg.weightedSims;
      sampleSize = agg.weightedSims;
    } else if (agg.unweightedCount > 0) {
      winWhite = agg.unweightedWin / agg.unweightedCount;
      sampleSize = agg.unweightedCount;
    }

    let winStats = null;
    if (agg.winStatsWeightedSims > 0 || agg.winStatsUnweightedCount > 0) {
      const denom = agg.winStatsWeightedSims > 0 ? agg.winStatsWeightedSims : agg.winStatsUnweightedCount;
      const sums = agg.winStatsWeightedSims > 0 ? agg.winStatsWeightedSums : agg.winStatsUnweightedSums;
      const values = {};
      for (const metric of HOLDOUT_WIN_STAT_METRICS) {
        values[metric.suffix] = sums[metric.suffix] / denom;
      }
      winStats = {
        leaderColor: agg.winStatsLeaderColor,
        values,
      };
    }

    branches.push({
      settlement1: agg.s1,
      road1: agg.r1,
      settlement2: agg.s2,
      road2: agg.r2,
      winWhite,
      sampleSize,
      followers: bestFollowerVariant(agg.followerVariants),
      winStats,
    });
  }

  branches.sort(compareBranchPlacement);

  const groupsByPair = new Map();
  for (const branch of branches) {
    const a = Math.min(branch.settlement1, branch.settlement2);
    const b = Math.max(branch.settlement1, branch.settlement2);
    const pairKey = `${a}|${b}`;

    let group = groupsByPair.get(pairKey);
    if (!group) {
      group = {
        settlementA: a,
        settlementB: b,
        branches: [],
      };
      groupsByPair.set(pairKey, group);
    }
    group.branches.push(branch);
  }

  const groups = [];
  for (const group of groupsByPair.values()) {
    group.branches.sort(compareBranchPlacement);
    const bestBranch = group.branches[0];
    groups.push({
      settlementA: group.settlementA,
      settlementB: group.settlementB,
      bestBranch,
      branches: group.branches,
      branchCount: group.branches.length,
      winWhite: bestBranch.winWhite,
      sampleSize: bestBranch.sampleSize,
      winStats: bestBranch.winStats || null,
    });
  }

  groups.sort((a, b) => {
    if (b.winWhite !== a.winWhite) return b.winWhite - a.winWhite;
    if (a.settlementA !== b.settlementA) return a.settlementA - b.settlementA;
    return a.settlementB - b.settlementB;
  });

  return {
    topGroups: groups.slice(0, 5),
    hasEnhancedRows,
  };
}

async function loadHoldoutTopPicks(sample) {
  const sampleId = String(sample?.id || "").trim();
  if (!sampleId) {
    return { available: false, message: "No sample id; cannot resolve holdout file." };
  }

  const relPath = `${sampleId}/${HOLDOUT_CSV_BASENAME}`;
  if (holdoutCache.has(relPath)) {
    return holdoutCache.get(relPath);
  }

  try {
    const csvText = await fetchAnalysisText(relPath);
    const rows = parseCsv(csvText);
    const holdoutSummary = buildHoldoutTopPicks(rows);
    const result = {
      available: true,
      sampleId,
      relPath,
      topGroups: holdoutSummary.topGroups,
      isEnhancedSample: holdoutSummary.hasEnhancedRows,
    };
    holdoutCache.set(relPath, result);
    return result;
  } catch (error) {
    return {
      available: false,
      message: error instanceof Error ? error.message : String(error),
    };
  }
}

function sampleLabel(sample, idx) {
  const id = sample.id || String(idx + 1).padStart(4, "0");
  return `${id} - ${sample.board_file}`;
}

function resourceAt(tileResources, tileIndex) {
  const resource = tileResources[tileIndex];
  return resource === null ? "DESERT" : resource;
}

function expandNumbers(tileResources, compactNumbers) {
  const out = new Array(tileResources.length).fill(null);
  let ptr = 0;
  for (let i = 0; i < tileResources.length; i += 1) {
    if (tileResources[i] === null) continue;
    out[i] = compactNumbers[ptr] ?? null;
    ptr += 1;
  }
  return out;
}

function letterByTile(tileResources) {
  const out = new Array(tileResources.length).fill(null);
  let ptr = 0;
  for (const tile of SERPENT_TILE_ORDER) {
    if (tileResources[tile] === null) continue;
    out[tile] = TOKEN_LETTERS[ptr] || null;
    ptr += 1;
  }
  return out;
}

function axialToPixel(x, z, size, cx, cy) {
  const q = x;
  const r = z;
  const px = size * Math.sqrt(3) * (q + r / 2);
  const py = size * 1.5 * r;
  return [cx + px, cy + py];
}

function hexPath(cx, cy, size) {
  const pts = [];
  for (let i = 0; i < 6; i += 1) {
    const ang = (60 * i - 30) * Math.PI / 180;
    const x = cx + size * Math.cos(ang);
    const y = cy + size * Math.sin(ang);
    pts.push(`${x.toFixed(2)},${y.toFixed(2)}`);
  }
  return pts.join(" ");
}

function computeNodePositions(size, centerX, centerY) {
  const radius = size - 2;
  const nodeSums = new Map();
  const nodeCounts = new Map();
  for (let tile = 0; tile < TILE_COORDS.length; tile += 1) {
    const [x, , z] = TILE_COORDS[tile];
    const [cx, cy] = axialToPixel(x, z, size, centerX, centerY);
    for (let local = 0; local < 6; local += 1) {
      const nodeId = TILE_NODES[tile][local];
      const vertex = (local + 5) % 6;
      const ang = (60 * vertex - 30) * Math.PI / 180;
      const px = cx + radius * Math.cos(ang);
      const py = cy + radius * Math.sin(ang);
      const prev = nodeSums.get(nodeId) || [0, 0];
      nodeSums.set(nodeId, [prev[0] + px, prev[1] + py]);
      nodeCounts.set(nodeId, (nodeCounts.get(nodeId) || 0) + 1);
    }
  }
  const out = [];
  for (const [nodeId, sum] of nodeSums.entries()) {
    const n = nodeCounts.get(nodeId) || 1;
    out[nodeId] = [sum[0] / n, sum[1] / n];
  }
  return out;
}

function renderPortsOnBoard(board, nodePositions, centerX, centerY) {
  const ports = board.port_resources || [];
  for (let i = 0; i < PORT_NODE_PAIRS.length; i += 1) {
    const pair = PORT_NODE_PAIRS[i];
    const a = nodePositions[pair[0]];
    const b = nodePositions[pair[1]];
    if (!a || !b) continue;

    const midX = (a[0] + b[0]) / 2;
    const midY = (a[1] + b[1]) / 2;
    const vx = midX - centerX;
    const vy = midY - centerY;
    const norm = Math.hypot(vx, vy) || 1;
    const tagX = midX + (vx / norm) * 24;
    const tagY = midY + (vy / norm) * 24;

    const linkA = document.createElementNS("http://www.w3.org/2000/svg", "line");
    linkA.setAttribute("class", "port-link");
    linkA.setAttribute("x1", tagX.toFixed(2));
    linkA.setAttribute("y1", tagY.toFixed(2));
    linkA.setAttribute("x2", a[0].toFixed(2));
    linkA.setAttribute("y2", a[1].toFixed(2));
    svg.appendChild(linkA);

    const linkB = document.createElementNS("http://www.w3.org/2000/svg", "line");
    linkB.setAttribute("class", "port-link");
    linkB.setAttribute("x1", tagX.toFixed(2));
    linkB.setAttribute("y1", tagY.toFixed(2));
    linkB.setAttribute("x2", b[0].toFixed(2));
    linkB.setAttribute("y2", b[1].toFixed(2));
    svg.appendChild(linkB);

    const label = ports[i] === null ? "3:1" : ports[i];
    const rectW = Math.max(34, 8 + label.length * 7.4);
    const rectH = 24;
    const rectX = tagX - rectW / 2;
    const rectY = tagY - rectH / 2;

    const bubble = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    bubble.setAttribute("class", "port-tag");
    bubble.setAttribute("x", rectX.toFixed(2));
    bubble.setAttribute("y", rectY.toFixed(2));
    bubble.setAttribute("width", rectW.toFixed(2));
    bubble.setAttribute("height", rectH.toFixed(2));
    bubble.setAttribute("rx", "12");
    svg.appendChild(bubble);

    const txt = document.createElementNS("http://www.w3.org/2000/svg", "text");
    txt.setAttribute("class", "port-label");
    txt.setAttribute("x", tagX.toFixed(2));
    txt.setAttribute("y", (tagY + 0.5).toFixed(2));
    txt.textContent = label;
    svg.appendChild(txt);
  }
}

function drawPlacementForColor(nodePositions, token, colorName, placement, options = {}) {
  if (!placement) return;
  const target = options.target || svg;
  const roadClass = options.roadClass || "placement-road";
  const settlementClass = options.settlementClass || "placement-settlement";
  const labelClass = options.labelClass || "placement-label";
  const haloClass = options.haloClass || "";
  const roadWidth = options.roadWidth || 8;
  const haloRadius = options.haloRadius || 14;
  const settlementRadius = options.settlementRadius || 11;
  const haloFill = options.haloFill || "rgba(255, 255, 255, 0.9)";
  const haloStroke = options.haloStroke || "#0f172a";
  const haloStrokeWidth = options.haloStrokeWidth || "2";
  const stroke = PLAYER_STROKE[colorName] || "#334155";

  if (placement.road && placement.road.length === 2) {
    const a = nodePositions[placement.road[0]];
    const b = nodePositions[placement.road[1]];
    if (a && b) {
      const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
      line.setAttribute("class", roadClass);
      line.setAttribute("x1", a[0].toFixed(2));
      line.setAttribute("y1", a[1].toFixed(2));
      line.setAttribute("x2", b[0].toFixed(2));
      line.setAttribute("y2", b[1].toFixed(2));
      line.setAttribute("stroke", stroke);
      line.setAttribute("stroke-width", String(roadWidth));
      target.appendChild(line);
    }
  }

  if (placement.settlement !== undefined && placement.settlement !== null) {
    const p = nodePositions[placement.settlement];
    if (p) {
      const halo = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      if (haloClass) halo.setAttribute("class", haloClass);
      halo.setAttribute("cx", p[0].toFixed(2));
      halo.setAttribute("cy", p[1].toFixed(2));
      halo.setAttribute("r", String(haloRadius));
      halo.setAttribute("fill", haloFill);
      halo.setAttribute("stroke", haloStroke);
      halo.setAttribute("stroke-width", String(haloStrokeWidth));
      target.appendChild(halo);

      const c = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      c.setAttribute("class", settlementClass);
      c.setAttribute("cx", p[0].toFixed(2));
      c.setAttribute("cy", p[1].toFixed(2));
      c.setAttribute("r", String(settlementRadius));
      c.setAttribute("fill", "#ffffff");
      c.setAttribute("stroke", stroke);
      c.setAttribute("stroke-width", "5");
      target.appendChild(c);

      const t = document.createElementNS("http://www.w3.org/2000/svg", "text");
      t.setAttribute("class", labelClass);
      t.setAttribute("x", p[0].toFixed(2));
      t.setAttribute("y", (p[1] + 0.4).toFixed(2));
      t.setAttribute("fill", stroke);
      t.textContent = token;
      target.appendChild(t);
    }
  }
}

function renderPlacementsOnBoard(placements, nodePositions) {
  if (!placements) return;
  drawPlacementForColor(nodePositions, "R1", "RED", placements.red1);
  drawPlacementForColor(nodePositions, "B1", "BLUE", placements.blue1);
  drawPlacementForColor(nodePositions, "O1", "ORANGE", placements.orange1);
}

function shortColor(colorName) {
  if (colorName === "ORANGE") return "O";
  if (colorName === "WHITE") return "W";
  if (colorName === "BLUE") return "B";
  if (colorName === "RED") return "R";
  return String(colorName || "?").slice(0, 1).toUpperCase();
}

function renderHoverPickOnBoard(nodePositions, pick) {
  if (!pick) return;

  const entries = [
    { color: "WHITE", settlement: pick.settlement1, road: pick.road1 },
    { color: "WHITE", settlement: pick.settlement2, road: pick.road2 },
    ...(pick.followers || []),
  ];

  const countByColor = {
    RED: 1,
    BLUE: 1,
    ORANGE: 1,
    WHITE: 0,
  };

  for (const entry of entries) {
    const color = entry.color;
    const next = (countByColor[color] || 0) + 1;
    countByColor[color] = next;
    const token = `${shortColor(color)}${next}`;
    drawPlacementForColor(
      nodePositions,
      token,
      color,
      { settlement: entry.settlement, road: entry.road },
      {
        roadClass: "preview-road",
        settlementClass: "preview-settlement",
        labelClass: "preview-label",
        haloClass: "preview-halo",
        roadWidth: 9,
        haloRadius: 15,
        settlementRadius: 11,
        haloFill: "rgba(255, 255, 255, 0.95)",
        haloStroke: "#0b1c2d",
        haloStrokeWidth: 2,
      }
    );
  }
}

function renderPorts(portResources) {
  portsEl.innerHTML = "";
  (portResources || []).forEach((port, idx) => {
    const chip = document.createElement("span");
    chip.className = "chip";
    chip.textContent = `${idx + 1}: ${port === null ? "3:1" : port}`;
    portsEl.appendChild(chip);
  });
}

function renderBoard(board, placements) {
  svg.innerHTML = "";
  const size = 52;
  const centerX = 380;
  const centerY = 305;
  const numbers = expandNumbers(board.tile_resources, board.numbers);
  const letters = letterByTile(board.tile_resources);

  for (let i = 0; i < TILE_COORDS.length; i += 1) {
    const [x, , z] = TILE_COORDS[i];
    const [cx, cy] = axialToPixel(x, z, size, centerX, centerY);
    const resource = resourceAt(board.tile_resources, i);
    const fill = RESOURCE_FILL[resource] || "#cccccc";

    const poly = document.createElementNS("http://www.w3.org/2000/svg", "polygon");
    poly.setAttribute("class", "hex");
    poly.setAttribute("points", hexPath(cx, cy, size - 2));
    poly.setAttribute("fill", fill);
    svg.appendChild(poly);

    const num = numbers[i];
    if (num !== null && num !== undefined) {
      const numText = document.createElementNS("http://www.w3.org/2000/svg", "text");
      numText.setAttribute("class", "tile-num");
      numText.setAttribute("x", cx.toFixed(2));
      numText.setAttribute("y", (cy + 1).toFixed(2));
      numText.setAttribute("fill", num === 6 || num === 8 ? "#9b0000" : "#111111");
      numText.textContent = String(num);
      svg.appendChild(numText);
    }

    if (letters[i]) {
      const idText = document.createElementNS("http://www.w3.org/2000/svg", "text");
      idText.setAttribute("class", "tile-id");
      idText.setAttribute("x", cx.toFixed(2));
      idText.setAttribute("y", (cy + size * 0.48).toFixed(2));
      idText.textContent = letters[i];
      svg.appendChild(idText);
    }
  }

  const nodePositions = computeNodePositions(size, centerX, centerY);
  renderPortsOnBoard(board, nodePositions, centerX, centerY);
  renderPlacementsOnBoard(placements, nodePositions);
  return nodePositions;
}

function renderActiveBoard() {
  if (!activeBoard) return null;
  const nodePositions = renderBoard(activeBoard, activePlacements);
  renderHoverPickOnBoard(nodePositions, activeHoverPick);
  fitBoardViewBoxToContent();
  return nodePositions;
}

function fitBoardViewBoxToContent() {
  if (!svg || svg.childElementCount === 0) return;
  const bbox = svg.getBBox();
  if (!Number.isFinite(bbox.width) || !Number.isFinite(bbox.height)) return;
  if (bbox.width <= 0 || bbox.height <= 0) return;
  const x = bbox.x - BOARD_VIEWBOX_PADDING;
  const y = bbox.y - BOARD_VIEWBOX_PADDING;
  const w = bbox.width + BOARD_VIEWBOX_PADDING * 2;
  const h = bbox.height + BOARD_VIEWBOX_PADDING * 2;
  svg.setAttribute("viewBox", `${x} ${y} ${w} ${h}`);
}

function resourceNumberTuplesForNode(board, numbers, nodeId) {
  const tileIndices = TILE_INDICES_BY_NODE[nodeId] || [];
  if (!tileIndices.length) return ["(UNKNOWN, -)"];
  return tileIndices.map((tileIndex) => {
    const resource = resourceAt(board.tile_resources, tileIndex);
    const number = numbers[tileIndex];
    return `(${resource}, ${number ?? "-"})`;
  });
}

function pipsForNumber(number) {
  if (!Number.isFinite(number)) return 0;
  return PIPS_BY_NUMBER[number] || 0;
}

function buildGroupPips(board, group) {
  const numbers = expandNumbers(board.tile_resources, board.numbers);
  const perResource = Object.fromEntries(RESOURCE_PIP_LABELS.map((entry) => [entry.resource, 0]));
  let total = 0;

  const settlements = [group.settlementA, group.settlementB];
  for (const settlement of settlements) {
    const tileIndices = TILE_INDICES_BY_NODE[settlement] || [];
    for (const tileIndex of tileIndices) {
      const resource = resourceAt(board.tile_resources, tileIndex);
      if (!Object.prototype.hasOwnProperty.call(perResource, resource)) continue;
      const pips = pipsForNumber(numbers[tileIndex]);
      perResource[resource] += pips;
      total += pips;
    }
  }

  return { perResource, total };
}

function createGroupPipsTable(board, group) {
  const pipStats = buildGroupPips(board, group);
  const table = document.createElement("table");
  table.className = "holdout-pips";

  const thead = document.createElement("thead");
  const headRow = document.createElement("tr");
  for (const entry of RESOURCE_PIP_LABELS) {
    const th = document.createElement("th");
    th.textContent = entry.label;
    headRow.appendChild(th);
  }
  const totalTh = document.createElement("th");
  totalTh.textContent = "TT";
  headRow.appendChild(totalTh);
  thead.appendChild(headRow);
  table.appendChild(thead);

  const tbody = document.createElement("tbody");
  const valueRow = document.createElement("tr");
  for (const entry of RESOURCE_PIP_LABELS) {
    const td = document.createElement("td");
    td.textContent = String(pipStats.perResource[entry.resource]);
    valueRow.appendChild(td);
  }
  const totalTd = document.createElement("td");
  totalTd.textContent = String(pipStats.total);
  valueRow.appendChild(totalTd);
  tbody.appendChild(valueRow);
  table.appendChild(tbody);

  return table;
}

function roadDirectionFromSettlement(nodePositions, settlement, road) {
  if (!road || road.length !== 2) return "unknown";
  const [a, b] = road;
  let target = null;
  if (settlement === a) target = b;
  else if (settlement === b) target = a;
  else target = b;

  const from = nodePositions[settlement];
  const to = nodePositions[target];
  if (!from || !to) return "unknown";

  const dx = to[0] - from[0];
  const dy = to[1] - from[1];
  const mag = Math.hypot(dx, dy);
  if (mag < 1e-6) return "unknown";

  const ux = dx / mag;
  const uy = dy / mag;
  let best = ROAD_DIRECTION_VECTORS[0].name;
  let bestScore = -Infinity;
  for (const candidate of ROAD_DIRECTION_VECTORS) {
    const score = ux * candidate.x + uy * candidate.y;
    if (score > bestScore) {
      best = candidate.name;
      bestScore = score;
    }
  }
  return best;
}

function formatSampleSize(value) {
  if (!Number.isFinite(value)) return "0";
  const rounded = Math.round(value);
  if (Math.abs(value - rounded) < 1e-6) return String(rounded);
  return value.toFixed(1);
}

function formatHoldoutWinStat(metric, value) {
  if (!Number.isFinite(value)) return "-";
  const numeric = value.toFixed(metric.decimals);
  if (metric.format === "percent") return `${numeric}%`;
  return numeric;
}

function refreshHoldoutCardActiveState() {
  if (!holdoutList) return;
  const cards = holdoutList.querySelectorAll(".holdout-card");
  for (const card of cards) {
    const idx = Number(card.getAttribute("data-pick-idx"));
    card.classList.toggle("active", Number.isInteger(idx) && idx === activeHoverPickIndex);
  }
  const branches = holdoutList.querySelectorAll(".holdout-branch");
  for (const branch of branches) {
    const branchKey = String(branch.getAttribute("data-branch-key") || "");
    branch.classList.toggle("active", branchKey.length > 0 && branchKey === activeHoverBranchKey);
  }
}

function setHoldoutMessage(message) {
  if (!holdoutList) return;
  holdoutList.innerHTML = "";
  const p = document.createElement("p");
  p.className = "muted";
  p.textContent = message;
  holdoutList.appendChild(p);
}

function setActiveHoldoutPreview(pick, groupIdx, branchKey) {
  activeHoverPick = pick;
  activeHoverPickIndex = groupIdx;
  activeHoverBranchKey = branchKey || null;
  renderActiveBoard();
  refreshHoldoutCardActiveState();
}

function clearActiveHoldoutPreview(groupIdx, branchKey = null) {
  if (activeHoverPickIndex !== groupIdx) return;
  if (branchKey !== null && activeHoverBranchKey !== branchKey) return;
  activeHoverPick = null;
  activeHoverPickIndex = null;
  activeHoverBranchKey = null;
  renderActiveBoard();
  refreshHoldoutCardActiveState();
}

function bindGroupCardHover(card, bestBranch, groupIdx) {
  card.addEventListener("mouseenter", () => setActiveHoldoutPreview(bestBranch, groupIdx, null));
  card.addEventListener("mouseleave", () => clearActiveHoldoutPreview(groupIdx));
  card.addEventListener("focus", () => setActiveHoldoutPreview(bestBranch, groupIdx, null));
  card.addEventListener("blur", () => clearActiveHoldoutPreview(groupIdx));
  card.tabIndex = 0;
}

function bindBranchHover(branchEl, branch, groupIdx, branchKey) {
  branchEl.addEventListener("mouseenter", () => setActiveHoldoutPreview(branch, groupIdx, branchKey));
  branchEl.addEventListener("focus", () => setActiveHoldoutPreview(branch, groupIdx, branchKey));
  branchEl.addEventListener("blur", () => clearActiveHoldoutPreview(groupIdx, branchKey));
  branchEl.tabIndex = 0;
}

function createBranchListItem(branch, groupIdx, branchIdx, nodePositions, options = {}) {
  const showWin = options.showWin !== false;
  const item = document.createElement("div");
  const branchKey = `${groupIdx}:${branchIdx}`;
  item.className = "holdout-branch";
  item.setAttribute("data-branch-key", branchKey);

  const s1RoadDir = roadDirectionFromSettlement(nodePositions, branch.settlement1, branch.road1);
  const s2RoadDir = roadDirectionFromSettlement(nodePositions, branch.settlement2, branch.road2);

  const placement = document.createElement("span");
  placement.className = "holdout-branch-order";
  placement.textContent = `S1 ${branch.settlement1} (${s1RoadDir}) | S2 ${branch.settlement2} (${s2RoadDir})`;

  item.appendChild(placement);
  if (showWin) {
    const win = document.createElement("span");
    win.className = "holdout-branch-win";
    win.textContent = `${branch.winWhite.toFixed(1)}%`;
    item.appendChild(win);
  }
  bindBranchHover(item, branch, groupIdx, branchKey);
  return item;
}

function createHoldoutCardBase(group, idx, board, nodePositions) {
  const pick = group.bestBranch;
  const card = document.createElement("article");
  card.className = "holdout-card";
  card.setAttribute("data-pick-idx", String(idx));

  const header = document.createElement("div");
  header.className = "holdout-card-title";

  const rankEl = document.createElement("span");
  rankEl.className = "holdout-rank";
  rankEl.textContent = `#${idx + 1}`;

  const winEl = document.createElement("span");
  winEl.className = "holdout-win";
  winEl.textContent = `WHITE ${pick.winWhite.toFixed(1)}%`;

  header.appendChild(rankEl);
  header.appendChild(winEl);
  card.appendChild(header);

  const branchesHead = document.createElement("p");
  branchesHead.className = "holdout-branches-label";
  branchesHead.textContent = "Branch win rates";
  card.appendChild(branchesHead);
  card.appendChild(createGroupPipsTable(board, group));

  const branchesPreview = document.createElement("div");
  branchesPreview.className = "holdout-branch-list";
  const previewCount = Math.min(1, group.branches.length);
  for (let branchIdx = 0; branchIdx < previewCount; branchIdx += 1) {
    branchesPreview.appendChild(
      createBranchListItem(group.branches[branchIdx], idx, branchIdx, nodePositions, { showWin: false })
    );
  }
  card.appendChild(branchesPreview);

  if (group.branches.length > previewCount) {
    const details = document.createElement("details");
    details.className = "holdout-branches-more";

    const worstBranch = group.branches[group.branches.length - 1];
    const worstText = worstBranch ? ` | Worst: ${worstBranch.winWhite.toFixed(1)}%` : "";
    const summary = document.createElement("summary");
    summary.textContent = `Show ${group.branches.length - previewCount} more branches${worstText}`;
    details.appendChild(summary);

    const moreList = document.createElement("div");
    moreList.className = "holdout-branch-list holdout-branch-list-more";
    for (let branchIdx = previewCount; branchIdx < group.branches.length; branchIdx += 1) {
      moreList.appendChild(createBranchListItem(group.branches[branchIdx], idx, branchIdx, nodePositions));
    }
    details.appendChild(moreList);
    card.appendChild(details);
  }

  bindGroupCardHover(card, pick, idx);
  return card;
}

function createHoldoutCardLegacy(group, idx, board, nodePositions) {
  return createHoldoutCardBase(group, idx, board, nodePositions);
}

function createHoldoutCardEnhanced(group, idx, board, nodePositions) {
  const card = createHoldoutCardBase(group, idx, board, nodePositions);
  if (!group.winStats || !group.winStats.values) return card;
  card.classList.add("holdout-card-enhanced");

  const layout = document.createElement("div");
  layout.className = "holdout-enhanced-layout";

  const mainCol = document.createElement("div");
  mainCol.className = "holdout-enhanced-main";
  while (card.firstChild) {
    mainCol.appendChild(card.firstChild);
  }

  const statsCol = document.createElement("aside");
  statsCol.className = "holdout-stats holdout-stats-right";

  for (const suffix of HOLDOUT_WIN_STAT_DISPLAY_SUFFIXES) {
    const metric = HOLDOUT_WIN_STAT_BY_SUFFIX.get(suffix);
    if (!metric) continue;

    const statEl = document.createElement("div");
    statEl.className = "holdout-stat-compact";

    const labelEl = document.createElement("span");
    labelEl.className = "holdout-stat-compact-label";
    labelEl.textContent = metric.label;

    const valueEl = document.createElement("span");
    valueEl.className = "holdout-stat-compact-value";
    valueEl.textContent = formatHoldoutWinStat(metric, group.winStats.values[suffix]);

    statEl.appendChild(labelEl);
    statEl.appendChild(valueEl);
    statsCol.appendChild(statEl);
  }

  layout.appendChild(mainCol);
  layout.appendChild(statsCol);
  card.appendChild(layout);
  return card;
}

function setHoldoutEnhancedMode(enabled) {
  if (!wrapEl) return;
  wrapEl.classList.toggle("holdout-enhanced", enabled);
}

async function renderHoldoutTopPicks(sample, board, nodePositions) {
  if (!holdoutList) return;
  setHoldoutEnhancedMode(false);
  setHoldoutMessage("Loading holdout analysis...");

  const holdout = await loadHoldoutTopPicks(sample);
  if (!holdout.available) {
    setHoldoutMessage(`Holdout analysis unavailable for sample ${sample.id}: ${holdout.message}`);
    return;
  }

  if (!holdout.topGroups.length) {
    setHoldoutMessage(`No usable holdout rows for sample ${sample.id}.`);
    return;
  }

  const enhancedSample = Boolean(holdout.isEnhancedSample);
  setHoldoutEnhancedMode(enhancedSample);

  holdoutList.innerHTML = "";
  holdout.topGroups.forEach((group, idx) => {
    const card = (enhancedSample && group.winStats)
      ? createHoldoutCardEnhanced(group, idx, board, nodePositions)
      : createHoldoutCardLegacy(group, idx, board, nodePositions);
    holdoutList.appendChild(card);
  });
  refreshHoldoutCardActiveState();
}

function updateControls() {
  prevBtn.disabled = current <= 0;
  nextBtn.disabled = current >= samples.length - 1;
  sampleSelect.value = String(current);
}

function fillSampleSelect() {
  sampleSelect.innerHTML = "";
  samples.forEach((sample, idx) => {
    const option = document.createElement("option");
    option.value = String(idx);
    option.textContent = sampleLabel(sample, idx);
    sampleSelect.appendChild(option);
  });
}

async function refreshIndex(keepSelection) {
  const oldSample = samples[current];
  const text = await fetchText("index.json");
  if (text === indexTextCache && samples.length > 0) {
    return false;
  }

  indexTextCache = text;
  indexPayload = parseJson(text, "index.json");
  samples = indexPayload.samples || [];

  if (!samples.length) {
    sampleSelect.innerHTML = "";
    setStatus("No boards found in data folder.");
    return true;
  }

  fillSampleSelect();

  if (keepSelection && oldSample) {
    const oldId = oldSample.id;
    const oldBoard = oldSample.board_file;
    const maybeIndex = samples.findIndex((s) => (oldId && s.id === oldId) || s.board_file === oldBoard);
    current = maybeIndex >= 0 ? maybeIndex : Math.min(current, samples.length - 1);
  } else {
    current = Math.min(current, samples.length - 1);
  }

  updateControls();
  return true;
}

function placementFromSample(sample, state) {
  return state?.placements || {
    red1: sample.red1 || null,
    blue1: sample.blue1 || null,
    orange1: sample.orange1 || null,
  };
}

async function loadCurrentSample() {
  const sample = samples[current];
  if (!sample) return;
  loadInProgress = true;

  try {
    setStatus(`Loading sample ${current + 1}/${samples.length}...`);

    const boardText = await fetchText(sample.board_file);
    boardTextCache = boardText;
    const board = parseJson(boardText, sample.board_file);

    let state = null;
    if (sample.state_file) {
      try {
        const stateText = await fetchText(sample.state_file);
        stateTextCache = stateText;
        state = parseJson(stateText, sample.state_file);
      } catch (_) {
        stateTextCache = null;
        state = null;
      }
    } else {
      stateTextCache = null;
    }

    const placements = placementFromSample(sample, state);
    activeBoard = board;
    activePlacements = placements;
    activeHoverPick = null;
    activeHoverPickIndex = null;

    const nodePositions = renderActiveBoard();
    renderPorts(board.port_resources || []);
    await renderHoldoutTopPicks(sample, board, nodePositions);

    metaText.textContent = JSON.stringify(
      {
        sample_id: sample.id,
        board_file: sample.board_file,
        state_file: sample.state_file || null,
        board_seed: sample.board_seed ?? null,
        bot_seed: sample.bot_seed ?? null,
      },
      null,
      2,
    );

    placementsText.textContent = JSON.stringify(placements, null, 2);
    updateControls();
    setStatus(`Showing sample ${current + 1}/${samples.length}`);
  } finally {
    loadInProgress = false;
  }
}

async function autoRefreshTick() {
  if (loadInProgress) return;
  if (!samples.length) return;

  try {
    const indexChanged = await refreshIndex(true);
    const sample = samples[current];
    if (!sample) return;

    let shouldReload = indexChanged;
    if (!shouldReload) {
      const nextBoardText = await fetchText(sample.board_file);
      if (nextBoardText !== boardTextCache) {
        shouldReload = true;
      }

      if (sample.state_file) {
        const nextStateText = await fetchText(sample.state_file);
        if (nextStateText !== stateTextCache) {
          shouldReload = true;
        }
      } else if (stateTextCache !== null) {
        shouldReload = true;
      }
    }

    if (shouldReload) {
      await loadCurrentSample();
    }
  } catch (_) {
    // Ignore polling errors and keep current rendering.
  }
}

function startAutoRefresh() {
  if (pollTimer) {
    clearInterval(pollTimer);
  }
  pollTimer = setInterval(() => {
    void autoRefreshTick();
  }, POLL_MS);
}

function bindControls() {
  prevBtn.addEventListener("click", async () => {
    if (current <= 0) return;
    current -= 1;
    await loadCurrentSample();
  });

  nextBtn.addEventListener("click", async () => {
    if (current >= samples.length - 1) return;
    current += 1;
    await loadCurrentSample();
  });

  sampleSelect.addEventListener("change", async (event) => {
    current = Number(event.target.value);
    await loadCurrentSample();
  });

  document.addEventListener("keydown", async (event) => {
    if (event.key === "ArrowLeft" && current > 0) {
      current -= 1;
      await loadCurrentSample();
    } else if (event.key === "ArrowRight" && current < samples.length - 1) {
      current += 1;
      await loadCurrentSample();
    }
  });
}

async function init() {
  dataPathEl.textContent = `data: ${DATA_ROOT.pathname} | analysis: ${ANALYSIS_ROOT.pathname}`;
  bindControls();

  try {
    await refreshIndex(false);
    if (!samples.length) return;
    await loadCurrentSample();
    startAutoRefresh();
  } catch (error) {
    setStatus(`Error: ${error.message}`);
  }
}

void init();
