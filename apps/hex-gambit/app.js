const app = document.getElementById("app");
const panelViewport = document.getElementById("panel-viewport");

let fitRaf = 0;

const BOARD_DATA_URL = "./data/boards.json";
const HEX_SIZE = 44;
const VIEW_PADDING = 4;
const SQRT3 = 1.73205080757;

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

const TOKEN_WEIGHT = {
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
  error: null,
};

let MODEL = null;
const sequenceRankCache = new Map();

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
  state.stage = "placement";
  state.boardIndex = 0;
  state.stepIndex = 0;
  state.selections = MODEL.boards.map((board) => ({ ...board.seedSelection }));
  state.boardResults = [];
  state.signals = [];
  state.showMeta = false;
  render();
}

function nodeLandTileIds(board, nodeId) {
  return MODEL.nodeToLandTileIds[nodeId].filter((tileId) => {
    const tile = board.tiles[tileId];
    return tile.type === "RESOURCE_TILE" || tile.type === "DESERT";
  });
}

function nodeResourceStats(boardIndex, nodeId) {
  const board = MODEL.boards[boardIndex];
  const landTileIds = nodeLandTileIds(board, nodeId);
  let ows = 0;
  let total = 0;
  const resources = new Set();

  for (const tileId of landTileIds) {
    const tile = board.tiles[tileId];
    if (tile.type === "DESERT") {
      continue;
    }
    const weight = TOKEN_WEIGHT[tile.number] || 0;
    resources.add(tile.resource);
    total += weight;
    if (tile.resource === "ORE" || tile.resource === "WHEAT" || tile.resource === "SHEEP") {
      ows += weight;
    }
  }

  return {
    ows,
    total,
    diversity: resources.size,
  };
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

function nodeRoadPotential(nodeId, occupiedNodeIds) {
  const neighbors = MODEL.neighborsByNode[nodeId];
  let free = 0;
  for (const n of neighbors) {
    if (!occupiedNodeIds.has(n)) free += 1;
  }
  return free + neighbors.size * 0.25;
}

function settlementMetrics(boardIndex, nodeId, occupiedNodeIds) {
  const stats = nodeResourceStats(boardIndex, nodeId);
  const roadPotential = nodeRoadPotential(nodeId, occupiedNodeIds);
  return {
    ows: stats.ows * 2 + stats.total * 0.4,
    road: roadPotential * 2 + stats.diversity * 0.8,
    rank: stats.ows * 2.2 + stats.total * 1.15 + stats.diversity * 1.3 + roadPotential * 0.9,
  };
}

function roadTarget(edgeId, anchorNodeId) {
  const [a, b] = MODEL.edgesById[edgeId].id;
  return a === anchorNodeId ? b : a;
}

function roadMetrics(boardIndex, edgeId, anchorNodeId, occupiedNodeIds) {
  const targetNodeId = roadTarget(edgeId, anchorNodeId);
  const targetStats = nodeResourceStats(boardIndex, targetNodeId);
  const roadPotential = nodeRoadPotential(targetNodeId, occupiedNodeIds);
  const edgeHexBonus = MODEL.edgesById[edgeId].landTileCount;

  return {
    ows: targetStats.ows * 2 + edgeHexBonus * 0.4,
    road: roadPotential * 2.2 + edgeHexBonus * 0.9,
    rank:
      targetStats.ows * 1.6 +
      targetStats.total * 0.9 +
      roadPotential * 1.7 +
      edgeHexBonus * 1.1,
  };
}

function optionMetrics(boardIndex, selection, step, optionId) {
  const board = MODEL.boards[boardIndex];
  const occupiedNodeIds = occupiedNodes(selection, board);

  if (step.type === "settlement") {
    return settlementMetrics(boardIndex, optionId, occupiedNodeIds);
  }

  const anchor = step.key === "e1" ? selection.s1 : selection.s2;
  if (anchor === null) return { ows: 0, road: 0, rank: 0 };
  return roadMetrics(boardIndex, optionId, anchor, occupiedNodeIds);
}

function normalize(value, min, max) {
  if (max <= min) return 1;
  return (value - min) / (max - min);
}

function inferSignal(boardIndex, selection, step, legal, chosen) {
  const scored = legal.map((optionId) => ({
    optionId,
    metrics: optionMetrics(boardIndex, selection, step, optionId),
  }));
  const chosenEntry = scored.find((entry) => entry.optionId === chosen);
  if (!chosenEntry) return "rank";

  const mins = { ows: Infinity, road: Infinity, rank: Infinity };
  const maxs = { ows: -Infinity, road: -Infinity, rank: -Infinity };
  for (const entry of scored) {
    mins.ows = Math.min(mins.ows, entry.metrics.ows);
    mins.road = Math.min(mins.road, entry.metrics.road);
    mins.rank = Math.min(mins.rank, entry.metrics.rank);
    maxs.ows = Math.max(maxs.ows, entry.metrics.ows);
    maxs.road = Math.max(maxs.road, entry.metrics.road);
    maxs.rank = Math.max(maxs.rank, entry.metrics.rank);
  }

  const normalized = {
    ows: normalize(chosenEntry.metrics.ows, mins.ows, maxs.ows),
    road: normalize(chosenEntry.metrics.road, mins.road, maxs.road),
    rank: normalize(chosenEntry.metrics.rank, mins.rank, maxs.rank),
  };

  let winner = SIGNAL_PRECEDENCE[0];
  for (const signal of SIGNAL_PRECEDENCE) {
    if (normalized[signal] > normalized[winner]) winner = signal;
  }
  return winner;
}

function sequenceKey(selection) {
  return `${selection.s1}|${selection.e1}|${selection.s2}|${selection.e2}`;
}

function sequenceScore(boardIndex, selection) {
  if (selection.s1 === null || selection.e1 === null || selection.s2 === null || selection.e2 === null) {
    return -Infinity;
  }

  const board = MODEL.boards[boardIndex];
  const baseOccupied = new Set(board.baseOccupiedNodeIds);
  const occupiedS1 = new Set(baseOccupied);
  occupiedS1.add(selection.s1);
  const occupiedBoth = new Set(occupiedS1);
  occupiedBoth.add(selection.s2);

  const s1 = settlementMetrics(boardIndex, selection.s1, baseOccupied);
  const r1 = roadMetrics(boardIndex, selection.e1, selection.s1, occupiedS1);
  const s2 = settlementMetrics(boardIndex, selection.s2, occupiedS1);
  const r2 = roadMetrics(boardIndex, selection.e2, selection.s2, occupiedBoth);

  const stats1 = nodeResourceStats(boardIndex, selection.s1);
  const stats2 = nodeResourceStats(boardIndex, selection.s2);
  const n1 = MODEL.nodesById[selection.s1];
  const n2 = MODEL.nodesById[selection.s2];
  const spread = Math.hypot(n1.x - n2.x, n1.y - n2.y) / HEX_SIZE;

  return (
    s1.rank * 1.0 +
    r1.rank * 0.7 +
    s2.rank * 1.1 +
    r2.rank * 0.8 +
    (stats1.ows + stats2.ows) * 1.5 +
    (stats1.diversity + stats2.diversity) * 0.6 +
    spread * 0.6
  );
}

function compareTuple(a, b) {
  if (a.s1 !== b.s1) return a.s1 - b.s1;
  if (a.e1 !== b.e1) return a.e1 - b.e1;
  if (a.s2 !== b.s2) return a.s2 - b.s2;
  return a.e2 - b.e2;
}

function buildRankCache(boardIndex) {
  if (sequenceRankCache.has(boardIndex)) return sequenceRankCache.get(boardIndex);

  const sequences = [];
  const stepFlow = MODEL.stepFlow;
  const seed = { ...MODEL.boards[boardIndex].seedSelection };

  function dfs(stepIndex, selection) {
    if (stepIndex >= stepFlow.length) {
      sequences.push({
        selection: { ...selection },
        key: sequenceKey(selection),
        score: sequenceScore(boardIndex, selection),
      });
      return;
    }

    const step = stepFlow[stepIndex];
    if (selection[step.key] !== null) {
      dfs(stepIndex + 1, selection);
      return;
    }

    const options = legalOptions(boardIndex, selection, step);
    for (const optionId of options) {
      const next = { ...selection, [step.key]: optionId };
      dfs(stepIndex + 1, next);
    }
  }

  dfs(0, seed);

  sequences.sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score;
    return compareTuple(a.selection, b.selection);
  });

  const rankByKey = new Map();
  sequences.forEach((entry, index) => {
    rankByKey.set(entry.key, index + 1);
  });

  const cache = { rankByKey, total: sequences.length };
  sequenceRankCache.set(boardIndex, cache);
  return cache;
}

function boardResult(boardIndex, selection) {
  const cache = buildRankCache(boardIndex);
  if (cache.total === 0) return { rank: 1, total: 1, winPct: 0 };
  const rank = cache.rankByKey.get(sequenceKey(selection)) || cache.total;
  const percentile = 1 - (rank - 1) / Math.max(1, cache.total - 1);
  const winPct = Number((32 + percentile * 36).toFixed(1));
  return { rank, total: cache.total, winPct };
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
    state.boardResults[state.boardIndex] = boardResult(state.boardIndex, selection);
    state.stage = "board_result";
  } else {
    state.stage = "placement";
  }
  render();
}

function continueFlow() {
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

function renderBoardSvg(boardIndex, step, legalSet, interactive) {
  const board = MODEL.boards[boardIndex];
  const selection = state.selections[boardIndex];
  const youColor = MODEL.perspectiveColor;

  const placedNodeColorById = new Map();
  board.basePlacedNodes.forEach((n) => placedNodeColorById.set(n.id, n.color));
  if (selection.s1 !== null) placedNodeColorById.set(selection.s1, youColor);
  if (selection.s2 !== null) placedNodeColorById.set(selection.s2, youColor);

  const placedEdgeColorByKey = new Map();
  board.basePlacedEdges.forEach((e) => placedEdgeColorByKey.set(edgeKey(e.id[0], e.id[1]), e.color));
  if (selection.e1 !== null) {
    const edge = MODEL.edgesById[selection.e1];
    placedEdgeColorByKey.set(edgeKey(edge.id[0], edge.id[1]), youColor);
  }
  if (selection.e2 !== null) {
    const edge = MODEL.edgesById[selection.e2];
    placedEdgeColorByKey.set(edgeKey(edge.id[0], edge.id[1]), youColor);
  }

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
    const token =
      tile.type === "RESOURCE_TILE" && tile.number
        ? `<text class="hex-token" x="${tileGeom.vcx}" y="${tileGeom.vcy + 5}">${tile.number}</text>`
        : "";
    tiles.push(`
      <g class="hex-cell">
        <polygon points="${tileGeom.vpoints}" fill="${fill}" />
        ${token}
      </g>
    `);
  });

  const renderableNodeIds = new Set(
    MODEL.nodes
      .filter((node) => MODEL.nodeToLandTileIds[node.id].length > 0 || placedNodeColorById.has(node.id))
      .map((node) => node.id)
  );

  const renderableEdgeIds = new Set(
    MODEL.edges
      .filter((edge) => {
        const key = edgeKey(edge.id[0], edge.id[1]);
        const isPlaced = placedEdgeColorByKey.has(key);
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
    const placedColor = placedEdgeColorByKey.get(key);
    if (placedColor) classes.push("placed", `color-${COLOR_CLASS[placedColor]}`);
    const clickable = interactive && step.type === "road" && legalSet.has(edge.idIndex);
    if (!placedColor && clickable) classes.push("legal", "clickable");
    if (!placedColor && interactive && step.type === "road" && !clickable) classes.push("idle");
    const hitTarget = clickable
      ? `<line class="board-edge-hit" x1="${a.vx}" y1="${a.vy}" x2="${b.vx}" y2="${b.vy}" data-edge-id="${edge.idIndex}" />`
      : "";
    return `
      <line
        class="${classes.join(" ")}"
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
    const placedColor = placedNodeColorById.get(node.id);
    if (placedColor) classes.push("placed", `color-${COLOR_CLASS[placedColor]}`);
    const clickable = interactive && step.type === "settlement" && legalSet.has(node.id);
    if (!placedColor && clickable) classes.push("legal", "clickable");
    if (!placedColor && interactive && step.type === "settlement" && !clickable) classes.push("idle");
    const radius = placedColor ? 13.5 : clickable ? 7 : 4.5;
    const hitTarget = clickable
      ? `<circle class="board-node-hit" cx="${node.vx}" cy="${node.vy}" r="16" data-node-id="${node.id}" />`
      : "";
    return `
      <circle
        class="${classes.join(" ")}"
        cx="${node.vx}" cy="${node.vy}" r="${radius}"
      />
      ${hitTarget}
    `;
    });

  const nodeTags = MODEL.nodes
    .filter((node) => placedNodeColorById.has(node.id) && renderableNodeIds.has(node.id))
    .map((node) => {
      const color = placedNodeColorById.get(node.id);
      const short = PLAYER_SHORT[color] || "?";
      return `<text class="player-tag color-${COLOR_CLASS[color]}" x="${node.vx}" y="${node.vy + 3.5}">${short}</text>`;
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
    <p class="lead">Failed to load board data.</p>
    <div class="inline-note">${state.error || "Unknown error"}</div>
  `;
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

  app.innerHTML = `
    <div class="summary-chip">Board ${state.boardIndex + 1} Result</div>
    <h2 class="card-title">${MODEL.boards[state.boardIndex].label} complete</h2>
    ${renderLegend(true)}
    <div class="board-wrap">${renderBoardSvg(state.boardIndex, step, legalSet, false)}</div>
    <div class="scoreline">
      <strong>Selected win percentage:</strong> ${result.winPct}%<br/>
      <strong>Global rank:</strong> ${result.rank} / ${result.total}
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
  const boards = payload.boards.map((board) => {
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
    sequenceRankCache.clear();
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
