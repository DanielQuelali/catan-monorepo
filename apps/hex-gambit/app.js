const app = document.getElementById("app");
const panelViewport = document.getElementById("panel-viewport");

let fitRaf = 0;

const BOARD_DATA_URL = "./data/boards.json";
const ANALYSIS_ROOT_URL = "./runtime-data/opening_states";
const HOLDOUT_CSV_BASENAME = "initial_branch_analysis_all_sims_holdout.csv";
const HEX_SIZE = 44;
const VIEW_PADDING = 4;
const SQRT3 = 1.73205080757;
const FOLLOWER_REVEAL_MS = 980;

const FULL_STEP_FLOW = [
  { key: "s1", type: "settlement", title: "Settlement 1", prompt: "Place Settlement 1." },
  { key: "e1", type: "road", title: "Road 1", prompt: "Place Road 1 connected to Settlement 1." },
  {
    key: "s2",
    type: "settlement",
    title: "Settlement 2",
    prompt: "Place Settlement 2 (distance rule still applies).",
  },
  { key: "e2", type: "road", title: "Road 2", prompt: "Place Road 2 connected to Settlement 2." },
];

const COLOR_CLASS = {
  RED: "red",
  BLUE: "blue",
  ORANGE: "orange",
  WHITE: "white",
};

const RESOURCE_COLOR = {
  WOOD: "#2f8f4c",
  BRICK: "#b84f3b",
  WHEAT: "#d1aa2d",
  SHEEP: "#6faf48",
  ORE: "#5d6474",
  DESERT: "#9f8c66",
  WATER: "#1a3353",
  PORT: "#245089",
};

const TOKEN_PIPS = {
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

const SIGNAL_PRECEDENCE = ["rank", "ows", "road"];
const PLAYSTYLE_BY_SIGNAL = {
  ows: "OWS Dev Card Specialist",
  road: "Road Network Architect",
  rank: "Top-Rank Absolutist",
};

const BADGE_BY_PLAYSTYLE = {
  "OWS Dev Card Specialist": "Dev Card Engine Badge",
  "Road Network Architect": "Route Builder Badge",
  "Top-Rank Absolutist": "Ladder Hunter Badge",
};

const WHY_BY_SIGNAL = {
  ows: "You repeatedly prioritized ore/wheat/sheep access and dev-card tempo.",
  road: "You repeatedly prioritized road shape and expansion lanes.",
  rank: "You repeatedly tracked the top-ranked continuation.",
};

const PLAYER_SHORT = {
  RED: "R",
  BLUE: "B",
  ORANGE: "O",
  WHITE: "W",
};

const OPENING_ORDER = [
  "RED S1",
  "RED R1",
  "BLUE S1",
  "BLUE R1",
  "ORANGE S1",
  "ORANGE R1",
  "WHITE S1",
  "WHITE R1",
  "WHITE S2",
  "WHITE R2",
  "ORANGE S2",
  "ORANGE R2",
  "BLUE S2",
  "BLUE R2",
  "RED S2",
  "RED R2",
];

const CUBE_DELTA_BY_DIRECTION = {
  EAST: [1, -1, 0],
  WEST: [-1, 1, 0],
  NORTHEAST: [1, 0, -1],
  NORTHWEST: [0, 1, -1],
  SOUTHEAST: [0, -1, 1],
  SOUTHWEST: [-1, 0, 1],
};

const OPPOSITE_DIRECTION = {
  EAST: "WEST",
  WEST: "EAST",
  NORTHEAST: "SOUTHWEST",
  NORTHWEST: "SOUTHEAST",
  SOUTHEAST: "NORTHWEST",
  SOUTHWEST: "NORTHEAST",
};

const CORNER_PAIR_BY_OUTWARD_DIRECTION = {
  NORTHEAST: [5, 0],
  EAST: [0, 1],
  SOUTHEAST: [1, 2],
  SOUTHWEST: [2, 3],
  WEST: [3, 4],
  NORTHWEST: [4, 5],
};

const state = {
  stage: "loading", // loading | intro | placement | board_result | summary | error
  boardIndex: 0,
  stepIndex: 0,
  selections: [],
  boardResults: [],
  signals: [],
  showMeta: false,
  resultReveal: null,
  error: null,
};

let MODEL = null;
let resultRevealTimer = 0;

function clearResultRevealTimer() {
  if (resultRevealTimer) {
    clearTimeout(resultRevealTimer);
    resultRevealTimer = 0;
  }
}

function beginBoardResultReveal(boardIndex) {
  clearResultRevealTimer();
  state.resultReveal = { boardIndex, showRates: false };
  resultRevealTimer = setTimeout(() => {
    if (state.stage !== "board_result" || state.boardIndex !== boardIndex) return;
    state.resultReveal = { boardIndex, showRates: true };
    render();
  }, FOLLOWER_REVEAL_MS);
}

function fitPanelToViewport() {
  if (!app || !panelViewport) return;

  app.style.transform = "none";

  const availableWidth = panelViewport.clientWidth;
  const availableHeight = panelViewport.clientHeight;
  const contentWidth = app.offsetWidth;
  const contentHeight = app.offsetHeight;
  if (!contentWidth || !contentHeight || !availableWidth || !availableHeight) return;

  const safeWidth = Math.max(0, availableWidth - 2);
  const safeHeight = Math.max(0, availableHeight - 2);
  const scale = Math.min(safeWidth / contentWidth, safeHeight / contentHeight, 1);
  const freeX = Math.max(0, availableWidth - contentWidth * scale);
  const freeY = Math.max(0, availableHeight - contentHeight * scale);
  const offsetX = freeX / 2;
  const offsetY = Math.min(freeY, Math.max(4, Math.min(14, freeY * 0.05)));
  app.style.transform = `translate(${offsetX}px, ${offsetY}px) scale(${scale})`;
}

function scheduleFit() {
  if (fitRaf) cancelAnimationFrame(fitRaf);
  fitRaf = requestAnimationFrame(() => {
    fitPanelToViewport();
  });
}

function cubeToAxial(cube) {
  return { q: cube[0], r: cube[2] };
}

function tileCenter(coordinate) {
  const axial = cubeToAxial(coordinate);
  return {
    x: HEX_SIZE * (SQRT3 * axial.q + (SQRT3 / 2) * axial.r),
    y: HEX_SIZE * 1.5 * axial.r,
  };
}

function getNodeDelta(direction, w, h) {
  switch (direction) {
    case "NORTH":
      return [0, -h / 2];
    case "NORTHEAST":
      return [w / 2, -h / 4];
    case "SOUTHEAST":
      return [w / 2, h / 4];
    case "SOUTH":
      return [0, h / 2];
    case "SOUTHWEST":
      return [-w / 2, h / 4];
    case "NORTHWEST":
      return [-w / 2, -h / 4];
    default:
      return [0, 0];
  }
}

function edgeKey(a, b) {
  return `${Math.min(a, b)}:${Math.max(a, b)}`;
}

function emptySelection() {
  return { s1: null, e1: null, s2: null, e2: null };
}

function startSession() {
  clearResultRevealTimer();
  state.stage = "placement";
  state.boardIndex = 0;
  state.stepIndex = 0;
  state.selections = MODEL.boards.map((board) => ({ ...board.seedSelection }));
  state.boardResults = [];
  state.signals = [];
  state.showMeta = false;
  state.resultReveal = null;
  render();
}

function occupiedNodes(selection, board) {
  const ids = new Set(board.baseOccupiedNodeIds);
  if (selection.s1 !== null) ids.add(selection.s1);
  if (selection.s2 !== null) ids.add(selection.s2);
  return ids;
}

function occupiedEdges(selection, board) {
  const ids = new Set(board.baseOccupiedEdgeKeys);
  if (selection.e1 !== null) ids.add(edgeKey(...MODEL.edgesById[selection.e1].id));
  if (selection.e2 !== null) ids.add(edgeKey(...MODEL.edgesById[selection.e2].id));
  return ids;
}

function forbiddenNodesFromOccupied(occupiedNodeIds) {
  const forbidden = new Set(occupiedNodeIds);
  for (const nodeId of occupiedNodeIds) {
    for (const neighbor of MODEL.neighborsByNode[nodeId]) {
      forbidden.add(neighbor);
    }
  }
  return forbidden;
}

function legalOptions(boardIndex, selection, step) {
  const board = MODEL.boards[boardIndex];
  const occupiedNodeIds = occupiedNodes(selection, board);
  const occupiedEdgeKeys = occupiedEdges(selection, board);
  const forbiddenNodes = forbiddenNodesFromOccupied(occupiedNodeIds);

  if (step.key === "s1") {
    return MODEL.validSettlementNodeIds.filter((nodeId) => !forbiddenNodes.has(nodeId));
  }

  if (step.key === "e1") {
    if (selection.s1 === null) return [];
    return MODEL.edgeIdsByNode[selection.s1].filter((edgeId) => {
      const key = edgeKey(...MODEL.edgesById[edgeId].id);
      return !occupiedEdgeKeys.has(key);
    });
  }

  if (step.key === "s2") {
    if (selection.s1 === null) return [];
    const extraForbidden = new Set([selection.s1, ...MODEL.neighborsByNode[selection.s1]]);
    return MODEL.validSettlementNodeIds.filter(
      (nodeId) => !forbiddenNodes.has(nodeId) && !extraForbidden.has(nodeId)
    );
  }

  if (step.key === "e2") {
    if (selection.s2 === null) return [];
    return MODEL.edgeIdsByNode[selection.s2].filter((edgeId) => {
      if (edgeId === selection.e1) return false;
      const key = edgeKey(...MODEL.edgesById[edgeId].id);
      return !occupiedEdgeKeys.has(key);
    });
  }

  return [];
}

function sequenceKey(selection) {
  return `${selection.s1}|${selection.e1}|${selection.s2}|${selection.e2}`;
}

function compareTuple(a, b) {
  if (a.s1 !== b.s1) return a.s1 - b.s1;
  if (a.e1 !== b.e1) return a.e1 - b.e1;
  if (a.s2 !== b.s2) return a.s2 - b.s2;
  return a.e2 - b.e2;
}

function splitCsvLine(line) {
  const out = [];
  let cur = "";
  let inQuotes = false;

  for (let i = 0; i < line.length; i += 1) {
    const ch = line[i];
    if (ch === "\"") {
      if (inQuotes && line[i + 1] === "\"") {
        cur += "\"";
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
  if (lines.length < 2) {
    throw new Error("analysis CSV is empty");
  }
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

function roadTokenToEdgeId(token) {
  const parts = String(token || "")
    .split("-")
    .map((value) => Number(value));
  if (parts.length !== 2 || !Number.isFinite(parts[0]) || !Number.isFinite(parts[1])) {
    return null;
  }
  const edgeId = MODEL.edgeIdByKey.get(edgeKey(parts[0], parts[1]));
  return edgeId === undefined ? null : edgeId;
}

function toInt(value) {
  const parsed = Number(value);
  return Number.isInteger(parsed) ? parsed : null;
}

function toNumber(value) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function parseFollowersFromRow(row) {
  const placements = [];
  for (let idx = 1; idx <= 4; idx += 1) {
    const colorRaw = row[`FOLLOWER${idx}_COLOR`];
    const settlementRaw = row[`FOLLOWER${idx}_SETTLEMENT`];
    const roadRaw = row[`FOLLOWER${idx}_ROAD`];
    if (colorRaw === undefined && settlementRaw === undefined && roadRaw === undefined) {
      break;
    }
    const color = String(colorRaw || "").trim().toUpperCase();
    const settlement = toInt(settlementRaw);
    const edgeId = roadTokenToEdgeId(roadRaw);
    if (!color || settlement === null || edgeId === null || !COLOR_CLASS[color]) continue;
    placements.push({ color, settlement, edgeId });
  }
  return placements;
}

function followerPlacementKey(followers) {
  return followers
    .map((placement) => `${placement.color}:${placement.settlement}:${placement.edgeId}`)
    .join("|");
}

function selectionMatchesPrefix(entry, prefix) {
  return (
    (prefix.s1 === null || prefix.s1 === entry.s1) &&
    (prefix.e1 === null || prefix.e1 === entry.e1) &&
    (prefix.s2 === null || prefix.s2 === entry.s2) &&
    (prefix.e2 === null || prefix.e2 === entry.e2)
  );
}

function bestContinuationWinPct(analysis, prefix) {
  let best = -Infinity;
  for (const entry of analysis.entries) {
    if (!selectionMatchesPrefix(entry.selection, prefix)) continue;
    if (entry.winPct > best) best = entry.winPct;
  }
  return best;
}

function bestOptionByContinuationWin(boardIndex, selection, step, legal) {
  const analysis = MODEL.boards[boardIndex].analysis;
  if (!analysis) return null;

  let bestOption = null;
  let bestWin = -Infinity;
  for (const optionId of legal) {
    const candidate = { ...selection, [step.key]: optionId };
    const win = bestContinuationWinPct(analysis, candidate);
    if (win > bestWin || (win === bestWin && (bestOption === null || optionId < bestOption))) {
      bestOption = optionId;
      bestWin = win;
    }
  }
  return bestOption;
}

function followsTopPrefix(boardIndex, selection, step, chosen) {
  const top = MODEL.boards[boardIndex].analysis?.topSelection;
  if (!top) return false;

  for (const flowStep of MODEL.stepFlow) {
    const key = flowStep.key;
    const value = key === step.key ? chosen : selection[key];
    if (value === null || value === undefined) break;
    if (value !== top[key]) return false;
    if (key === step.key) return true;
  }
  return false;
}

function inferSignal(boardIndex, selection, step, legal, chosen) {
  if (followsTopPrefix(boardIndex, selection, step, chosen)) return "rank";

  const bestOption = bestOptionByContinuationWin(boardIndex, selection, step, legal);
  if (bestOption === chosen) {
    return step.type === "road" ? "road" : "ows";
  }
  return step.type === "road" ? "road" : "ows";
}

function analysisCandidates(board) {
  if (typeof board.analysisPath === "string" && board.analysisPath.length > 0) {
    const base =
      board.analysisPath.endsWith(".csv") || board.analysisPath.endsWith(".csv.gz")
        ? board.analysisPath
        : `${board.analysisPath.replace(/\/$/, "")}/${HOLDOUT_CSV_BASENAME}`;
    if (base.endsWith(".csv.gz")) return [base, base.slice(0, -3)];
    if (base.endsWith(".csv")) return [`${base}.gz`, base];
    return [`${base}.gz`, base];
  }

  const analysisId = String(board.analysisId || "").trim();
  if (!analysisId) return [];
  const root = String(board.analysisRoot || ANALYSIS_ROOT_URL).replace(/\/$/, "");
  const base = `${root}/${analysisId}/${HOLDOUT_CSV_BASENAME}`;
  return [`${base}.gz`, base];
}

async function fetchAnalysisText(url) {
  const response = await fetch(url, { cache: "no-store" });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);

  if (!url.endsWith(".gz")) {
    return response.text();
  }

  const bytes = new Uint8Array(await response.arrayBuffer());
  const isGzip = bytes.length >= 2 && bytes[0] === 0x1f && bytes[1] === 0x8b;
  if (!isGzip) {
    // Some servers auto-decompress .gz responses; treat payload as plain CSV text.
    return new TextDecoder().decode(bytes);
  }

  if (typeof DecompressionStream === "undefined") {
    throw new Error("gzip unsupported");
  }
  const ds = new DecompressionStream("gzip");
  return new Response(new Blob([bytes]).stream().pipeThrough(ds)).text();
}

function buildBoardAnalysis(csvText) {
  const rows = parseCsv(csvText);
  const aggregates = new Map();
  const playerColors =
    rows.length > 0
      ? Object.keys(rows[0])
          .filter((header) => header.startsWith("WIN_"))
          .map((header) => header.slice(4))
          .filter((color) => COLOR_CLASS[color])
      : ["RED", "BLUE", "ORANGE", "WHITE"];

  for (const row of rows) {
    const s1 = toInt(row.LEADER_SETTLEMENT);
    const e1 = roadTokenToEdgeId(row.LEADER_ROAD);
    const s2 = toInt(row.LEADER_SETTLEMENT2);
    const e2 = roadTokenToEdgeId(row.LEADER_ROAD2);
    const winPctByColor = {};
    for (const color of playerColors) {
      const value = toNumber(row[`WIN_${color}`]);
      winPctByColor[color] = value;
    }
    const simsRun = toNumber(row.SIMS_RUN);
    const followers = parseFollowersFromRow(row);

    if (
      s1 === null ||
      e1 === null ||
      s2 === null ||
      e2 === null ||
      winPctByColor.WHITE === null
    ) {
      continue;
    }

    const key = `${s1}|${e1}|${s2}|${e2}`;
    let agg = aggregates.get(key);
    if (!agg) {
      const weightedWinPctByColor = {};
      const unweightedWinPctByColor = {};
      for (const color of playerColors) {
        weightedWinPctByColor[color] = 0;
        unweightedWinPctByColor[color] = 0;
      }
      agg = {
        selection: { s1, e1, s2, e2 },
        weightedSims: 0,
        unweightedCount: 0,
        weightedWinPctByColor,
        unweightedWinPctByColor,
        followerVariants: new Map(),
      };
    }

    if (simsRun !== null && simsRun > 0) {
      agg.weightedSims += simsRun;
      for (const color of playerColors) {
        const value = winPctByColor[color];
        if (value === null) continue;
        agg.weightedWinPctByColor[color] += value * simsRun;
      }
    } else {
      agg.unweightedCount += 1;
      for (const color of playerColors) {
        const value = winPctByColor[color];
        if (value === null) continue;
        agg.unweightedWinPctByColor[color] += value;
      }
    }

    if (followers.length > 0) {
      const variantKey = followerPlacementKey(followers);
      const variant = agg.followerVariants.get(variantKey) || {
        followers,
        weightedSims: 0,
        unweightedCount: 0,
      };
      if (simsRun !== null && simsRun > 0) variant.weightedSims += simsRun;
      else variant.unweightedCount += 1;
      agg.followerVariants.set(variantKey, variant);
    }
    aggregates.set(key, agg);
  }

  const entries = [];
  for (const [key, agg] of aggregates.entries()) {
    const winPctByColor = {};
    for (const color of playerColors) {
      const value =
        agg.weightedSims > 0
          ? agg.weightedWinPctByColor[color] / agg.weightedSims
          : agg.unweightedCount > 0
            ? agg.unweightedWinPctByColor[color] / agg.unweightedCount
            : 0;
      winPctByColor[color] = value;
    }

    let followers = [];
    for (const variant of agg.followerVariants.values()) {
      if (followers.length === 0) {
        followers = variant.followers;
        continue;
      }
      const currentKey = followerPlacementKey(followers);
      const candidateKey = followerPlacementKey(variant.followers);
      const current = agg.followerVariants.get(currentKey);
      if (!current) {
        followers = variant.followers;
        continue;
      }
      if (variant.weightedSims > current.weightedSims) {
        followers = variant.followers;
        continue;
      }
      if (variant.weightedSims === current.weightedSims) {
        if (variant.unweightedCount > current.unweightedCount) {
          followers = variant.followers;
          continue;
        }
        if (variant.unweightedCount === current.unweightedCount && candidateKey < currentKey) {
          followers = variant.followers;
        }
      }
    }

    entries.push({
      key,
      selection: agg.selection,
      winPct: winPctByColor.WHITE ?? 0,
      winPctByColor,
      simsRun: agg.weightedSims,
      followers,
    });
  }
  if (entries.length === 0) {
    throw new Error("analysis CSV had no usable WHITE12 rows");
  }

  entries.sort((a, b) => {
    if (b.winPct !== a.winPct) return b.winPct - a.winPct;
    return compareTuple(a.selection, b.selection);
  });

  const rankByKey = new Map();
  const entryByKey = new Map();
  entries.forEach((entry, idx) => {
    rankByKey.set(entry.key, idx + 1);
    entryByKey.set(entry.key, entry);
  });

  return {
    entries,
    rankByKey,
    entryByKey,
    total: entries.length,
    topSelection: entries[0].selection,
  };
}

async function loadBoardAnalysis(board) {
  const candidates = analysisCandidates(board);
  if (candidates.length === 0) {
    throw new Error(`board ${board.id} has no analysis source configured`);
  }

  let lastError = "unknown";
  for (const candidate of candidates) {
    try {
      const csvText = await fetchAnalysisText(candidate);
      return buildBoardAnalysis(csvText);
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
    }
  }

  throw new Error(
    `failed to load analysis for board ${board.id}; tried ${candidates.join(", ")}; last error: ${lastError}`
  );
}

async function hydrateBoardAnalyses() {
  for (const board of MODEL.boards) {
    board.analysis = await loadBoardAnalysis(board);
  }
}

function boardResult(boardIndex, selection) {
  const board = MODEL.boards[boardIndex];
  const analysis = board.analysis;
  if (!analysis) {
    throw new Error("analysis data unavailable for board result");
  }
  const key = sequenceKey(selection);
  const entry = analysis.entryByKey.get(key);
  if (!entry) {
    throw new Error(
      `Missing analysis result for board ${board.id} sequence ${key}. Refusing to show default 0%.`
    );
  }
  const rank = analysis.rankByKey.get(key) || analysis.total;
  const winPctByColor = {};
  for (const [color, value] of Object.entries(entry.winPctByColor || {})) {
    winPctByColor[color] = Number(value.toFixed(1));
  }
  if (winPctByColor.WHITE === undefined) {
    winPctByColor.WHITE = Number(entry.winPct.toFixed(1));
  }
  return {
    rank,
    total: analysis.total,
    winPct: Number((winPctByColor.WHITE ?? entry.winPct).toFixed(1)),
    winPctByColor,
    followers: Array.isArray(entry.followers) ? entry.followers : [],
  };
}

function signalCounts() {
  return state.signals.reduce(
    (acc, signal) => {
      acc[signal] += 1;
      return acc;
    },
    { ows: 0, road: 0, rank: 0 }
  );
}

function winnerSignal() {
  const counts = signalCounts();
  let winner = SIGNAL_PRECEDENCE[0];
  for (const signal of SIGNAL_PRECEDENCE) {
    if (counts[signal] > counts[winner]) winner = signal;
  }
  return winner;
}

function choosePlacement(optionId) {
  const step = MODEL.stepFlow[state.stepIndex];
  const selection = state.selections[state.boardIndex];
  const legal = legalOptions(state.boardIndex, selection, step);
  if (!legal.includes(optionId)) return;

  const signal = inferSignal(state.boardIndex, selection, step, legal, optionId);
  state.signals.push(signal);
  selection[step.key] = optionId;
  state.stepIndex += 1;

  if (state.stepIndex >= MODEL.stepFlow.length) {
    try {
      state.boardResults[state.boardIndex] = boardResult(state.boardIndex, selection);
      state.stage = "board_result";
      beginBoardResultReveal(state.boardIndex);
    } catch (error) {
      clearResultRevealTimer();
      state.stage = "error";
      state.error = error instanceof Error ? error.message : String(error);
    }
  } else {
    state.stage = "placement";
  }
  render();
}

function continueFlow() {
  clearResultRevealTimer();
  state.resultReveal = null;
  if (state.boardIndex < MODEL.boards.length - 1) {
    state.boardIndex += 1;
    state.stepIndex = 0;
    state.stage = "placement";
  } else {
    state.stage = "summary";
  }
  render();
}

function rectanglePoints(center, ux, uy, length, width) {
  const halfLen = length / 2;
  const halfWidth = width / 2;
  const px = -uy;
  const py = ux;
  const p1 = { x: center.x - ux * halfLen - px * halfWidth, y: center.y - uy * halfLen - py * halfWidth };
  const p2 = { x: center.x + ux * halfLen - px * halfWidth, y: center.y + uy * halfLen - py * halfWidth };
  const p3 = { x: center.x + ux * halfLen + px * halfWidth, y: center.y + uy * halfLen + py * halfWidth };
  const p4 = { x: center.x - ux * halfLen + px * halfWidth, y: center.y - uy * halfLen + py * halfWidth };
  return `${p1.x},${p1.y} ${p2.x},${p2.y} ${p3.x},${p3.y} ${p4.x},${p4.y}`;
}

function buildPlankToNode(portCenter, node) {
  const dx = node.x - portCenter.x;
  const dy = node.y - portCenter.y;
  const len = Math.hypot(dx, dy);
  if (len < 8) return null;

  const ux = dx / len;
  const uy = dy / len;
  const startInset = 9.5;
  const endInset = 1.2;
  const start = {
    x: portCenter.x + ux * startInset,
    y: portCenter.y + uy * startInset,
  };
  const end = {
    x: node.x - ux * endInset,
    y: node.y - uy * endInset,
  };
  const plankLen = Math.hypot(end.x - start.x, end.y - start.y);
  const center = {
    x: (start.x + end.x) / 2,
    y: (start.y + end.y) / 2,
  };
  const plankWidth = 8.2;
  const points = rectanglePoints(center, ux, uy, plankLen, plankWidth);

  const px = -uy;
  const py = ux;
  const grooves = [0.32, 0.64].map((t) => {
    const gx = start.x + (end.x - start.x) * t;
    const gy = start.y + (end.y - start.y) * t;
    return {
      x1: gx - px * (plankWidth * 0.37),
      y1: gy - py * (plankWidth * 0.37),
      x2: gx + px * (plankWidth * 0.37),
      y2: gy + py * (plankWidth * 0.37),
    };
  });

  return { points, grooves };
}

function buildPortDock(cornerA, cornerB, portCenter) {
  const plankA = buildPlankToNode(portCenter, cornerA);
  const plankB = buildPlankToNode(portCenter, cornerB);
  const planks = [plankA, plankB].filter(Boolean);
  return planks.length ? { planks } : null;
}

function renderBoardSvg(boardIndex, step, legalSet, interactive, options = {}) {
  const board = MODEL.boards[boardIndex];
  const selection = state.selections[boardIndex];
  const youColor = MODEL.perspectiveColor;
  const followers = Array.isArray(options.followers) ? options.followers : [];
  const animateFollowers = Boolean(options.animateFollowers);

  const placedNodeMetaById = new Map();
  const setNodePlacement = (id, color, source = "base", delayMs = 0) => {
    placedNodeMetaById.set(id, { color, source, delayMs });
  };
  board.basePlacedNodes.forEach((n) => setNodePlacement(n.id, n.color, "base", 0));
  if (selection.s1 !== null) setNodePlacement(selection.s1, youColor, "you", 0);
  if (selection.s2 !== null) setNodePlacement(selection.s2, youColor, "you", 0);

  const placedEdgeMetaByKey = new Map();
  const setEdgePlacement = (a, b, color, source = "base", delayMs = 0) => {
    placedEdgeMetaByKey.set(edgeKey(a, b), { color, source, delayMs });
  };
  board.basePlacedEdges.forEach((e) => setEdgePlacement(e.id[0], e.id[1], e.color, "base", 0));
  if (selection.e1 !== null) {
    const edge = MODEL.edgesById[selection.e1];
    setEdgePlacement(edge.id[0], edge.id[1], youColor, "you", 0);
  }
  if (selection.e2 !== null) {
    const edge = MODEL.edgesById[selection.e2];
    setEdgePlacement(edge.id[0], edge.id[1], youColor, "you", 0);
  }
  followers.forEach((placement, idx) => {
    const nodeDelayMs = idx * 180;
    const edgeDelayMs = idx * 180 + 90;
    const edge = MODEL.edgesById[placement.edgeId];
    if (Number.isInteger(placement.settlement)) {
      setNodePlacement(
        placement.settlement,
        placement.color,
        "follower",
        animateFollowers ? nodeDelayMs : 0
      );
    }
    if (edge) {
      setEdgePlacement(
        edge.id[0],
        edge.id[1],
        placement.color,
        "follower",
        animateFollowers ? edgeDelayMs : 0
      );
    }
  });

  const tiles = [];
  const ports = [];
  MODEL.tiles.forEach((tileGeom) => {
    const tile = board.tiles[tileGeom.id];
    if (tile.type === "PORT") {
      const binding = MODEL.portBindingsByTileId[tileGeom.id];
      if (!binding) return;

      const landTile = MODEL.tiles[binding.landTileId];
      const cornerA = landTile.vcorners[binding.cornerPair[0]];
      const cornerB = landTile.vcorners[binding.cornerPair[1]];
      const cx = tileGeom.vcx;
      const cy = tileGeom.vcy;
      const dock = buildPortDock(cornerA, cornerB, { x: cx, y: cy });
      if (!dock) return;

      const plankMarkup = dock.planks
        .map((plank, idx) => `<polygon class="port-dock port-dock-${idx === 0 ? "a" : "b"}" points="${plank.points}" />`)
        .join("");
      const dockSeams = dock.planks
        .flatMap((plank) => plank.grooves)
        .map(
          (line) =>
            `<line class="port-dock-seam" x1="${line.x1}" y1="${line.y1}" x2="${line.x2}" y2="${line.y2}" />`
        )
        .join("");
      const labelMarkup = tile.resource
        ? `
            <text class="port-label port-label-resource" x="${cx}" y="${cy - 5.2}">${labelResource(
            tile.resource
          ).toUpperCase()}</text>
            <text class="port-label port-label-ratio" x="${cx}" y="${cy + 8.6}">2:1</text>
          `
        : `<text class="port-label port-label-single" x="${cx}" y="${cy + 1.4}">3:1</text>`;
      ports.push(`
        <g class="port-marker">
          ${plankMarkup}
          ${dockSeams}
          ${labelMarkup}
        </g>
      `);
      return;
    }

    if (tile.type === "WATER") return;

    const fill =
      tile.type === "DESERT"
        ? RESOURCE_COLOR.DESERT
        : RESOURCE_COLOR[tile.resource];
    let token = "";
    if (tile.type === "RESOURCE_TILE" && tile.number) {
      const number = Number(tile.number);
      const hotToken = number === 6 || number === 8;
      const tokenClass = `hex-token${hotToken ? " hot" : ""}`;
      const pipCount = TOKEN_PIPS[number] || 0;
      const pipSpacing = 4.3;
      const pipStart = tileGeom.vcx - ((pipCount - 1) * pipSpacing) / 2;
      const pipY = tileGeom.vcy + 13.8;
      const pipClass = `hex-pip${hotToken ? " hot" : ""}`;
      const pips = Array.from({ length: pipCount }, (_, idx) => {
        const x = pipStart + idx * pipSpacing;
        return `<circle class="${pipClass}" cx="${x}" cy="${pipY}" r="1.5" />`;
      }).join("");
      token = `
        <g class="hex-token-wrap">
          <text class="${tokenClass}" x="${tileGeom.vcx}" y="${tileGeom.vcy + 5}">${tile.number}</text>
          ${pips}
        </g>
      `;
    }
    tiles.push(`
      <g class="hex-cell">
        <polygon points="${tileGeom.vpoints}" fill="${fill}" />
        ${token}
      </g>
    `);
  });

  const renderableNodeIds = new Set(
    MODEL.nodes
      .filter(
        (node) => MODEL.nodeToLandTileIds[node.id].length > 0 || placedNodeMetaById.has(node.id)
      )
      .map((node) => node.id)
  );

  const renderableEdgeIds = new Set(
    MODEL.edges
      .filter((edge) => {
        const key = edgeKey(edge.id[0], edge.id[1]);
        const isPlaced = placedEdgeMetaByKey.has(key);
        const isLegal = interactive && step.type === "road" && legalSet.has(edge.idIndex);
        return edge.landTileCount > 0 || isPlaced || isLegal;
      })
      .map((edge) => edge.idIndex)
  );

  const edges = MODEL.edges
    .filter((edge) => renderableEdgeIds.has(edge.idIndex))
    .map((edge) => {
    const a = MODEL.nodesById[edge.id[0]];
    const b = MODEL.nodesById[edge.id[1]];
    const key = edgeKey(edge.id[0], edge.id[1]);
    const classes = ["board-edge"];
    const placedMeta = placedEdgeMetaByKey.get(key);
    const placedColor = placedMeta?.color;
    if (placedColor) classes.push("placed", `color-${COLOR_CLASS[placedColor] || "white"}`);
    if (placedMeta?.source === "follower") classes.push("follower");
    if (placedMeta?.source === "follower" && animateFollowers) classes.push("follower-reveal");
    const clickable = interactive && step.type === "road" && legalSet.has(edge.idIndex);
    if (!placedColor && clickable) classes.push("legal", "clickable");
    if (!placedColor && interactive && step.type === "road" && !clickable) classes.push("idle");
    const inlineStyle =
      placedMeta?.source === "follower" && animateFollowers && placedMeta.delayMs > 0
        ? ` style="animation-delay: ${placedMeta.delayMs}ms;"`
        : "";
    const hitTarget = clickable
      ? `<line class="board-edge-hit" x1="${a.vx}" y1="${a.vy}" x2="${b.vx}" y2="${b.vy}" data-edge-id="${edge.idIndex}" />`
      : "";
    return `
      <line
        class="${classes.join(" ")}"
        ${inlineStyle}
        x1="${a.vx}" y1="${a.vy}"
        x2="${b.vx}" y2="${b.vy}"
      />
      ${hitTarget}
    `;
    });

  const nodes = MODEL.nodes
    .filter((node) => renderableNodeIds.has(node.id))
    .map((node) => {
    const classes = ["board-node"];
    const placedMeta = placedNodeMetaById.get(node.id);
    const placedColor = placedMeta?.color;
    if (placedColor) classes.push("placed", `color-${COLOR_CLASS[placedColor] || "white"}`);
    if (placedMeta?.source === "follower") classes.push("follower");
    if (placedMeta?.source === "follower" && animateFollowers) classes.push("follower-reveal");
    const clickable = interactive && step.type === "settlement" && legalSet.has(node.id);
    if (!placedColor && clickable) classes.push("legal", "clickable");
    if (!placedColor && interactive && step.type === "settlement" && !clickable) classes.push("idle");
    const radius = placedColor ? 13.5 : clickable ? 7 : 4.5;
    const inlineStyle =
      placedMeta?.source === "follower" && animateFollowers && placedMeta.delayMs > 0
        ? ` style="animation-delay: ${placedMeta.delayMs}ms;"`
        : "";
    const hitTarget = clickable
      ? `<circle class="board-node-hit" cx="${node.vx}" cy="${node.vy}" r="16" data-node-id="${node.id}" />`
      : "";
    return `
      <circle
        class="${classes.join(" ")}"
        ${inlineStyle}
        cx="${node.vx}" cy="${node.vy}" r="${radius}"
      />
      ${hitTarget}
    `;
    });

  const nodeTags = MODEL.nodes
    .filter((node) => placedNodeMetaById.has(node.id) && renderableNodeIds.has(node.id))
    .map((node) => {
      const placedMeta = placedNodeMetaById.get(node.id);
      const color = placedMeta?.color;
      if (!color) return "";
      const short = PLAYER_SHORT[color] || "?";
      const classes = ["player-tag", `color-${COLOR_CLASS[color] || "white"}`];
      if (placedMeta.source === "follower") classes.push("follower");
      if (placedMeta.source === "follower" && animateFollowers) classes.push("follower-reveal");
      const style =
        placedMeta.source === "follower" && animateFollowers && placedMeta.delayMs > 0
          ? ` style="animation-delay: ${placedMeta.delayMs + 70}ms;"`
          : "";
      return `<text class="${classes.join(" ")}"${style} x="${node.vx}" y="${node.vy + 3.5}">${short}</text>`;
    });

  const viewBox = MODEL.boardViewBox || { x: 0, y: 0, width: MODEL.width, height: MODEL.height };
  return `
    <svg
      class="board-svg"
      viewBox="${viewBox.x} ${viewBox.y} ${viewBox.width} ${viewBox.height}"
      preserveAspectRatio="xMidYMid meet"
    >
      ${tiles.join("")}
      ${ports.join("")}
      ${edges.join("")}
      ${nodes.join("")}
      ${nodeTags.join("")}
    </svg>
  `;
}

function bindBoardClicks() {
  app.querySelectorAll("[data-node-id]").forEach((nodeEl) => {
    nodeEl.addEventListener("click", () => {
      choosePlacement(Number(nodeEl.getAttribute("data-node-id")));
    });
  });
  app.querySelectorAll("[data-edge-id]").forEach((edgeEl) => {
    edgeEl.addEventListener("click", () => {
      choosePlacement(Number(edgeEl.getAttribute("data-edge-id")));
    });
  });
}

function labelResource(resource) {
  if (!resource) return "";
  return `${resource.slice(0, 1)}${resource.slice(1).toLowerCase()}`;
}

function compactTurnSegment() {
  return MODEL.stepFlow.map((step) => step.title.replace("Settlement", "S").replace("Road", "R")).join(" -> ");
}

function renderLegend(compact = false) {
  const you = MODEL.perspectiveColor;
  const tone = compact ? "compact" : "full";
  return `
    <div class="legend-row ${tone}">
      <span class="legend-chip red"><span class="dot"></span>RED${you === "RED" ? " (you)" : ""}</span>
      <span class="legend-chip blue"><span class="dot"></span>BLUE${you === "BLUE" ? " (you)" : ""}</span>
      <span class="legend-chip orange"><span class="dot"></span>ORANGE${you === "ORANGE" ? " (you)" : ""}</span>
      <span class="legend-chip white"><span class="dot"></span>WHITE${you === "WHITE" ? " (you)" : ""}</span>
    </div>
  `;
}

function renderStepRail(stepIndex) {
  const chips = MODEL.stepFlow.map((step, index) => {
    const classes = ["step-chip"];
    if (index < stepIndex) classes.push("done");
    if (index === stepIndex) classes.push("active");
    return `<span class="${classes.join(" ")}">${index + 1}. ${step.title}</span>`;
  });
  return `<div class="step-rail">${chips.join("")}</div>`;
}

function toggleMeta() {
  state.showMeta = !state.showMeta;
  render();
}

function renderOpeningOrder() {
  const suffixByKey = { s1: "S1", e1: "R1", s2: "S2", e2: "R2" };
  const activeEntries = new Set(
    MODEL.stepFlow.map((step) => `${MODEL.perspectiveColor} ${suffixByKey[step.key] || step.key}`)
  );
  const firstActive = OPENING_ORDER.findIndex((entry) => activeEntries.has(entry));
  const items = OPENING_ORDER.map((entry, index) => {
    const classes = ["order-chip"];
    if (firstActive > 0 && index < firstActive) classes.push("locked");
    if (activeEntries.has(entry)) classes.push("you");
    return `<span class="${classes.join(" ")}">${entry}</span>`;
  });
  return `<div class="opening-order">${items.join("")}</div>`;
}

function renderLoading() {
  app.innerHTML = `
    <h1 class="brand">Hex Gambit</h1>
    <p class="lead">Loading real board model...</p>
  `;
}

function renderError() {
  app.innerHTML = `
    <h1 class="brand">Hex Gambit</h1>
    <p class="lead">Hex Gambit encountered a data error.</p>
    <div class="inline-note">${state.error || "Unknown error"}</div>
    <div class="btn-row">
      <button class="btn btn-primary" id="restart">Restart Session</button>
    </div>
  `;
  const restartBtn = app.querySelector("#restart");
  if (restartBtn) restartBtn.addEventListener("click", startSession);
}

function renderIntro() {
  const turnSegment = MODEL.stepFlow.map((step) => `${MODEL.perspectiveColor} ${step.title}`).join(" -> ");
  app.innerHTML = `
    <h1 class="brand">Hex Gambit</h1>
    <p class="lead">We evaluate your real opening placements and reveal your playstyle at session end.</p>
    <p class="meta-row">Snake opening order is respected for a 4-player table.</p>
    ${renderOpeningOrder()}
    <p class="meta-row">Current turn segment: ${turnSegment}.</p>
    <p class="meta-row">All already-placed settlements/roads plus all ports are shown on every board.</p>
    ${renderLegend(false)}
    <div class="btn-row">
      <button class="btn btn-primary" id="start-session">Start Session</button>
    </div>
  `;
  app.querySelector("#start-session").addEventListener("click", startSession);
}

function renderPlacement() {
  const selection = state.selections[state.boardIndex];
  const step = MODEL.stepFlow[state.stepIndex];
  const legal = legalOptions(state.boardIndex, selection, step);
  const legalSet = new Set(legal);
  const totalSteps = MODEL.boards.length * MODEL.stepFlow.length;
  const stepNumber = state.boardIndex * MODEL.stepFlow.length + state.stepIndex + 1;
  const turnSegment = compactTurnSegment();
  const actionTarget = step.type === "settlement" ? "nodes" : "roads";
  const boardLabel = MODEL.boards[state.boardIndex].label;

  app.innerHTML = `
    <div class="control-bar">
      <span class="control-pill">Step ${stepNumber}/${totalSteps}</span>
      <span class="control-action">${step.title}</span>
      <span class="control-pill strong">Legal ${legal.length}</span>
      <button class="btn btn-ghost btn-mini" id="toggle-meta">${state.showMeta ? "Hide Info" : "Info"}</button>
      <button class="btn btn-ghost btn-mini icon" id="restart" aria-label="Restart">↺</button>
    </div>
    ${
      !state.showMeta
        ? ""
        : `
      <details class="meta-drawer">
        <summary>Table Info (Tap to collapse)</summary>
        <div class="drawer-body">
          <p class="drawer-line">${boardLabel}</p>
          <p class="drawer-line">Turn ${MODEL.perspectiveColor}: ${turnSegment}</p>
          ${renderLegend(true)}
        </div>
      </details>
    `
    }
    <div class="board-wrap focus">${renderBoardSvg(state.boardIndex, step, legalSet, true)}</div>
    <p class="micro-hint">Place on glowing ${actionTarget} only.</p>
  `;

  app.querySelector("#restart").addEventListener("click", startSession);
  const metaBtn = app.querySelector("#toggle-meta");
  if (metaBtn) metaBtn.addEventListener("click", toggleMeta);
  bindBoardClicks();
}

function renderBoardResult() {
  const result = state.boardResults[state.boardIndex];
  const step = MODEL.stepFlow[MODEL.stepFlow.length - 1];
  const legalSet = new Set();
  const reveal = state.resultReveal;
  const showRates = !reveal || reveal.boardIndex !== state.boardIndex || reveal.showRates;
  const bars = showRates
    ? Object.entries(result.winPctByColor || {})
        .filter(([color]) => COLOR_CLASS[color])
        .sort(([a], [b]) => {
          const order = { WHITE: 0, RED: 1, BLUE: 2, ORANGE: 3 };
          return (order[a] ?? 99) - (order[b] ?? 99);
        })
        .map(([color, value]) => {
          const pct = Number.isFinite(value) ? value : 0;
          const width = Math.max(0, Math.min(100, pct));
          return `
            <div class="tiny-win-row color-${COLOR_CLASS[color]}">
              <span class="tiny-win-label">${color}</span>
              <span class="tiny-win-track"><span class="tiny-win-fill color-${COLOR_CLASS[color]}" style="width:${width}%;"></span></span>
              <span class="tiny-win-value">${pct.toFixed(1)}%</span>
            </div>
          `;
        })
        .join("")
    : "";

  app.innerHTML = `
    <div class="summary-chip">Board ${state.boardIndex + 1} Result</div>
    <h2 class="card-title">${MODEL.boards[state.boardIndex].label} complete</h2>
    ${renderLegend(true)}
    <div class="board-wrap">${renderBoardSvg(state.boardIndex, step, legalSet, false, {
      followers: result.followers || [],
      animateFollowers: !showRates,
    })}</div>
    <div class="scoreline">
      ${
        showRates
          ? `
            <strong>Win rates (all players):</strong>
            <div class="tiny-win-bars">${bars}</div>
            <strong>Global rank:</strong> ${result.rank} / ${result.total}
          `
          : `<strong class="result-reveal-note">Followers are placing...</strong>`
      }
    </div>
    <div class="btn-row">
      <button class="btn btn-primary" id="continue">${
        state.boardIndex < MODEL.boards.length - 1
          ? `Continue to Board ${state.boardIndex + 2}`
          : "View Summary"
      }</button>
      <button class="btn btn-ghost" id="restart">Restart Session</button>
    </div>
  `;

  app.querySelector("#continue").addEventListener("click", continueFlow);
  app.querySelector("#restart").addEventListener("click", startSession);
}

function renderSummary() {
  const counts = signalCounts();
  const winner = winnerSignal();
  const playstyle = PLAYSTYLE_BY_SIGNAL[winner];
  const badge = BADGE_BY_PLAYSTYLE[playstyle];
  const avgRank = Math.round(
    state.boardResults.reduce((acc, r) => acc + r.rank, 0) / Math.max(1, state.boardResults.length)
  );
  const boardLines = state.boardResults
    .map((result, idx) => `<strong>Board ${idx + 1} rank:</strong> ${result.rank} / ${result.total}`)
    .join("<br/>");

  app.innerHTML = `
    <div class="summary-chip">Summary</div>
    <h2 class="card-title">${playstyle}</h2>
    <p class="prompt">${WHY_BY_SIGNAL[winner]}</p>
    <div class="kpi-grid">
      <div class="kpi"><div class="label">OWS/Dev picks</div><div class="value">${counts.ows} / ${
    MODEL.boards.length * MODEL.stepFlow.length
  }</div></div>
      <div class="kpi"><div class="label">Road picks</div><div class="value">${counts.road} / ${
    MODEL.boards.length * MODEL.stepFlow.length
  }</div></div>
      <div class="kpi"><div class="label">Top-rank picks</div><div class="value">${counts.rank} / ${
    MODEL.boards.length * MODEL.stepFlow.length
  }</div></div>
    </div>
    <div class="scoreline">${boardLines}<br/><strong>Average board rank:</strong> ${avgRank}</div>
    <div class="badge"><strong>Badge awarded:</strong> ${badge}</div>
    <div class="btn-row"><button class="btn btn-primary" id="restart">Restart Session</button></div>
  `;
  app.querySelector("#restart").addEventListener("click", startSession);
}

function applyStageClass() {
  const stageClasses = [
    "stage-loading",
    "stage-error",
    "stage-intro",
    "stage-placement",
    "stage-board_result",
    "stage-summary",
  ];
  stageClasses.forEach((name) => app.classList.remove(name));
  app.classList.add(`stage-${state.stage}`);
}

function render() {
  switch (state.stage) {
    case "loading":
      renderLoading();
      break;
    case "error":
      renderError();
      break;
    case "intro":
      renderIntro();
      break;
    case "placement":
      renderPlacement();
      break;
    case "board_result":
      renderBoardResult();
      break;
    case "summary":
      renderSummary();
      break;
    default:
      renderIntro();
      break;
  }
  applyStageClass();
  scheduleFit();
}

function buildModel(payload) {
  const boardModel = payload.boardModel;
  const perspectiveColor = payload.meta?.currentColor || payload.meta?.perspectiveColor || "WHITE";
  const stepByKey = Object.fromEntries(FULL_STEP_FLOW.map((step) => [step.key, step]));
  const metaSequenceKeys = Array.isArray(payload.meta?.sequenceKeys) ? payload.meta.sequenceKeys : [];
  const stepFlow =
    metaSequenceKeys
      .map((key) => stepByKey[key])
      .filter((step) => Boolean(step))
      .map((step) => ({ ...step })) || [];
  const activeStepFlow = stepFlow.length > 0 ? stepFlow : FULL_STEP_FLOW.map((step) => ({ ...step }));
  const boards = payload.boards.map((board, boardIndex) => {
    const normalizedTiles = board.tiles.map((entry) => {
      const tile = entry.tile;
      if (tile.type === "RESOURCE_TILE") {
        return {
          coordinate: entry.coordinate,
          type: "RESOURCE_TILE",
          resource: tile.resource,
          number: tile.number,
        };
      }
      if (tile.type === "DESERT") {
        return {
          coordinate: entry.coordinate,
          type: "DESERT",
        };
      }
      if (tile.type === "PORT") {
        return {
          coordinate: entry.coordinate,
          type: "PORT",
          resource: tile.resource || null,
          direction: tile.direction,
        };
      }
      if (tile.type === "WATER") {
        return {
          coordinate: entry.coordinate,
          type: "WATER",
        };
      }
      return {
        coordinate: entry.coordinate,
        type: tile.type || "WATER",
      };
    });

    const basePlacedNodes = board.basePlacedNodes.map((n) => ({
      id: n.id,
      color: n.color,
      building: n.building || "SETTLEMENT",
    }));
    const basePlacedEdges = board.basePlacedEdges.map((e) => ({
      id: [Math.min(e.id[0], e.id[1]), Math.max(e.id[0], e.id[1])],
      color: e.color,
    }));

    return {
      id: board.id,
      label: board.label,
      analysisId:
        (typeof board.analysis_id === "string" && board.analysis_id) ||
        (typeof board.analysisId === "string" && board.analysisId) ||
        String(boardIndex + 1).padStart(4, "0"),
      analysisPath:
        (typeof board.analysis_path === "string" && board.analysis_path) ||
        (typeof board.analysisPath === "string" && board.analysisPath) ||
        null,
      analysisRoot:
        (typeof board.analysis_root === "string" && board.analysis_root) ||
        (typeof board.analysisRoot === "string" && board.analysisRoot) ||
        null,
      analysis: null,
      seedSelection: board.seedSelection || null,
      tiles: normalizedTiles,
      basePlacedNodes,
      basePlacedEdges,
      baseOccupiedNodeIds: new Set(basePlacedNodes.map((n) => n.id)),
      baseOccupiedEdgeKeys: new Set(basePlacedEdges.map((e) => edgeKey(e.id[0], e.id[1]))),
    };
  });

  const nodes = boardModel.nodes.map((n) => ({
    id: n.id,
    tile_coordinate: n.tile_coordinate,
    direction: n.direction,
  }));
  const edges = boardModel.edges.map((e, idIndex) => ({
    idIndex,
    id: [Math.min(e.id[0], e.id[1]), Math.max(e.id[0], e.id[1])],
    tile_coordinate: e.tile_coordinate,
    direction: e.direction,
  }));

  const w = SQRT3 * HEX_SIZE;
  const h = HEX_SIZE * 2;

  const nodesById = {};
  for (const node of nodes) {
    const center = tileCenter(node.tile_coordinate);
    const [dx, dy] = getNodeDelta(node.direction, w, h);
    node.x = center.x + dx;
    node.y = center.y + dy;
    nodesById[node.id] = node;
  }

  const tiles = boards[0].tiles.map((tile, id) => {
    const center = tileCenter(tile.coordinate);
    const corners = [];
    for (let i = 0; i < 6; i += 1) {
      const angle = ((60 * i - 30) * Math.PI) / 180;
      corners.push({
        x: center.x + HEX_SIZE * Math.cos(angle),
        y: center.y + HEX_SIZE * Math.sin(angle),
      });
    }
    return {
      id,
      center,
      corners,
      coordinate: tile.coordinate,
    };
  });

  const tileIdByCoordKey = new Map();
  boards[0].tiles.forEach((tile, id) => {
    tileIdByCoordKey.set(tile.coordinate.join(","), id);
  });
  const portBindingsByTileId = {};
  boards[0].tiles.forEach((tile, id) => {
    if (tile.type !== "PORT") return;
    const delta = CUBE_DELTA_BY_DIRECTION[tile.direction];
    const opposite = OPPOSITE_DIRECTION[tile.direction];
    if (!delta || !opposite) return;
    const landCoord = [
      tile.coordinate[0] + delta[0],
      tile.coordinate[1] + delta[1],
      tile.coordinate[2] + delta[2],
    ];
    const landTileId = tileIdByCoordKey.get(landCoord.join(","));
    if (landTileId === undefined) return;
    const cornerPair = CORNER_PAIR_BY_OUTWARD_DIRECTION[opposite];
    if (!cornerPair) return;
    portBindingsByTileId[id] = { landTileId, cornerPair };
  });

  const neighborsByNode = {};
  const edgeIdsByNode = {};
  for (const node of nodes) {
    neighborsByNode[node.id] = new Set();
    edgeIdsByNode[node.id] = [];
  }
  for (const edge of edges) {
    const [a, b] = edge.id;
    neighborsByNode[a].add(b);
    neighborsByNode[b].add(a);
    edgeIdsByNode[a].push(edge.idIndex);
    edgeIdsByNode[b].push(edge.idIndex);
  }

  const landTileIds = boards[0].tiles
    .map((tile, idx) => ({ tile, idx }))
    .filter(({ tile }) => tile.type === "RESOURCE_TILE" || tile.type === "DESERT")
    .map(({ idx }) => idx);

  const nodeToLandTileIds = {};
  const CORNER_EPSILON = 1.1;
  for (const node of nodes) {
    const hits = [];
    for (const tileId of landTileIds) {
      const touchesTileCorner = tiles[tileId].corners.some(
        (corner) => Math.hypot(node.x - corner.x, node.y - corner.y) <= CORNER_EPSILON
      );
      if (touchesTileCorner) {
        hits.push(tileId);
      }
    }
    nodeToLandTileIds[node.id] = hits;
  }

  const validSettlementNodeIds = nodes
    .filter((node) => nodeToLandTileIds[node.id].length > 0)
    .map((node) => node.id);

  for (const edge of edges) {
    const tilesForA = new Set(nodeToLandTileIds[edge.id[0]]);
    const tilesForB = nodeToLandTileIds[edge.id[1]];
    let shared = 0;
    for (const t of tilesForB) {
      if (tilesForA.has(t)) shared += 1;
    }
    edge.landTileCount = shared;
  }

  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const tile of tiles) {
    for (const corner of tile.corners) {
      minX = Math.min(minX, corner.x);
      minY = Math.min(minY, corner.y);
      maxX = Math.max(maxX, corner.x);
      maxY = Math.max(maxY, corner.y);
    }
  }
  for (const node of nodes) {
    minX = Math.min(minX, node.x);
    minY = Math.min(minY, node.y);
    maxX = Math.max(maxX, node.x);
    maxY = Math.max(maxY, node.y);
  }

  const width = maxX - minX + VIEW_PADDING * 2;
  const height = maxY - minY + VIEW_PADDING * 2;
  const tx = (x) => x - minX + VIEW_PADDING;
  const ty = (y) => y - minY + VIEW_PADDING;

  nodes.forEach((node) => {
    node.vx = tx(node.x);
    node.vy = ty(node.y);
  });
  tiles.forEach((tile) => {
    tile.vcx = tx(tile.center.x);
    tile.vcy = ty(tile.center.y);
    tile.vcorners = tile.corners.map((p) => ({ x: tx(p.x), y: ty(p.y) }));
    tile.vpoints = tile.vcorners.map((p) => `${p.x},${p.y}`).join(" ");
  });

  const edgesById = {};
  edges.forEach((e) => {
    edgesById[e.idIndex] = e;
  });

  const edgeIdByKey = new Map();
  edges.forEach((edge) => {
    edgeIdByKey.set(edgeKey(edge.id[0], edge.id[1]), edge.idIndex);
  });

  boards.forEach((board) => {
    const seed = emptySelection();
    const ownNodes = board.basePlacedNodes.filter((node) => node.color === perspectiveColor).map((node) => node.id);
    const ownEdges = board.basePlacedEdges
      .filter((edge) => edge.color === perspectiveColor)
      .map((edge) => edgeIdByKey.get(edgeKey(edge.id[0], edge.id[1])))
      .filter((edgeId) => edgeId !== undefined);

    if (ownEdges.length > 0) {
      seed.e1 = ownEdges[0];
      const [a, b] = edgesById[ownEdges[0]].id;
      if (ownNodes.includes(a)) seed.s1 = a;
      else if (ownNodes.includes(b)) seed.s1 = b;
    }
    if (seed.s1 === null && ownNodes.length > 0) seed.s1 = ownNodes[0];

    if (board.seedSelection) {
      for (const key of ["s1", "e1", "s2", "e2"]) {
        if (board.seedSelection[key] !== null && board.seedSelection[key] !== undefined) {
          seed[key] = board.seedSelection[key];
        }
      }
    }

    board.seedSelection = seed;
  });

  const boardTileIds = boards[0].tiles
    .map((tile, idx) => ({ tile, idx }))
    .filter(({ tile }) => tile.type === "RESOURCE_TILE" || tile.type === "DESERT")
    .map(({ idx }) => idx);
  const portTileIds = boards[0].tiles
    .map((tile, idx) => ({ tile, idx }))
    .filter(({ tile }) => tile.type === "PORT")
    .map(({ idx }) => idx);

  let boardMinX = Infinity;
  let boardMinY = Infinity;
  let boardMaxX = -Infinity;
  let boardMaxY = -Infinity;
  boardTileIds.forEach((tileId) => {
    tiles[tileId].vcorners.forEach((corner) => {
      boardMinX = Math.min(boardMinX, corner.x);
      boardMinY = Math.min(boardMinY, corner.y);
      boardMaxX = Math.max(boardMaxX, corner.x);
      boardMaxY = Math.max(boardMaxY, corner.y);
    });
  });
  nodes.forEach((node) => {
    if (nodeToLandTileIds[node.id].length === 0) return;
    boardMinX = Math.min(boardMinX, node.vx);
    boardMinY = Math.min(boardMinY, node.vy);
    boardMaxX = Math.max(boardMaxX, node.vx);
    boardMaxY = Math.max(boardMaxY, node.vy);
  });
  portTileIds.forEach((tileId) => {
    const tile = tiles[tileId];
    boardMinX = Math.min(boardMinX, tile.vcx);
    boardMinY = Math.min(boardMinY, tile.vcy);
    boardMaxX = Math.max(boardMaxX, tile.vcx);
    boardMaxY = Math.max(boardMaxY, tile.vcy);
  });

  const boardViewPadding = 38;
  const boardViewBox =
    Number.isFinite(boardMinX) && Number.isFinite(boardMinY)
      ? {
          x: Math.max(0, boardMinX - boardViewPadding),
          y: Math.max(0, boardMinY - boardViewPadding),
          width:
            Math.min(width, boardMaxX + boardViewPadding) - Math.max(0, boardMinX - boardViewPadding),
          height:
            Math.min(height, boardMaxY + boardViewPadding) - Math.max(0, boardMinY - boardViewPadding),
        }
      : { x: 0, y: 0, width, height };

  return {
    perspectiveColor,
    stepFlow: activeStepFlow,
    boards,
    nodes,
    tiles,
    edges,
    nodesById,
    edgesById,
    edgeIdByKey,
    neighborsByNode,
    edgeIdsByNode,
    nodeToLandTileIds,
    portBindingsByTileId,
    validSettlementNodeIds,
    boardViewBox,
    width,
    height,
  };
}

async function loadBoards() {
  try {
    const response = await fetch(BOARD_DATA_URL);
    if (!response.ok) {
      throw new Error(`HTTP ${response.status} loading ${BOARD_DATA_URL}`);
    }
    const payload = await response.json();
    MODEL = buildModel(payload);
    await hydrateBoardAnalyses();
    state.selections = MODEL.boards.map((board) => ({ ...board.seedSelection }));
    state.stage = "intro";
    state.error = null;
  } catch (error) {
    state.stage = "error";
    state.error = error instanceof Error ? error.message : String(error);
  }
  render();
}

window.addEventListener("resize", scheduleFit, { passive: true });
window.addEventListener("orientationchange", scheduleFit, { passive: true });
if (document.fonts?.ready) {
  document.fonts.ready.then(scheduleFit).catch(() => {});
}

render();
loadBoards();
