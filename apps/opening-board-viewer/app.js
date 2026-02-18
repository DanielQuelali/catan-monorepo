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

const RESOURCE_FILL = {
  WOOD: "#4f9d69",
  BRICK: "#b75d44",
  SHEEP: "#9ac26a",
  WHEAT: "#d8bc59",
  ORE: "#7a7f8a",
  DESERT: "#d5bd8a",
};

const PLAYER_STROKE = {
  RED: "#c62828",
  BLUE: "#1565c0",
  ORANGE: "#ef6c00",
  WHITE: "#9aa1ad",
};

const POLL_MS = 1200;

const svg = document.getElementById("board");
const prevBtn = document.getElementById("prevBtn");
const nextBtn = document.getElementById("nextBtn");
const sampleSelect = document.getElementById("sampleSelect");
const statusEl = document.getElementById("status");
const dataPathEl = document.getElementById("dataPath");
const metaText = document.getElementById("metaText");
const portsEl = document.getElementById("ports");
const placementsText = document.getElementById("placementsText");

const params = new URLSearchParams(window.location.search);
const dataDirParam = params.get("data");
const DATA_ROOT = new URL(dataDirParam || "/data/opening_states/", window.location.href);

let samples = [];
let current = 0;
let indexPayload = null;
let indexTextCache = null;
let boardTextCache = null;
let stateTextCache = null;
let pollTimer = null;
let loadInProgress = false;

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

async function fetchText(relPath) {
  const url = withCacheBust(dataUrl(relPath));
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

function drawPlacementForColor(nodePositions, token, colorName, placement) {
  if (!placement) return;
  const stroke = PLAYER_STROKE[colorName] || "#334155";

  if (placement.road && placement.road.length === 2) {
    const a = nodePositions[placement.road[0]];
    const b = nodePositions[placement.road[1]];
    if (a && b) {
      const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
      line.setAttribute("class", "placement-road");
      line.setAttribute("x1", a[0].toFixed(2));
      line.setAttribute("y1", a[1].toFixed(2));
      line.setAttribute("x2", b[0].toFixed(2));
      line.setAttribute("y2", b[1].toFixed(2));
      line.setAttribute("stroke", stroke);
      line.setAttribute("stroke-width", "8");
      svg.appendChild(line);
    }
  }

  if (placement.settlement !== undefined && placement.settlement !== null) {
    const p = nodePositions[placement.settlement];
    if (p) {
      const halo = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      halo.setAttribute("cx", p[0].toFixed(2));
      halo.setAttribute("cy", p[1].toFixed(2));
      halo.setAttribute("r", "14");
      halo.setAttribute("fill", "rgba(255, 255, 255, 0.9)");
      halo.setAttribute("stroke", "#0f172a");
      halo.setAttribute("stroke-width", "2");
      svg.appendChild(halo);

      const c = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      c.setAttribute("class", "placement-settlement");
      c.setAttribute("cx", p[0].toFixed(2));
      c.setAttribute("cy", p[1].toFixed(2));
      c.setAttribute("r", "11");
      c.setAttribute("fill", "#ffffff");
      c.setAttribute("stroke", stroke);
      c.setAttribute("stroke-width", "5");
      svg.appendChild(c);

      const t = document.createElementNS("http://www.w3.org/2000/svg", "text");
      t.setAttribute("class", "placement-label");
      t.setAttribute("x", p[0].toFixed(2));
      t.setAttribute("y", (p[1] + 0.4).toFixed(2));
      t.setAttribute("fill", stroke);
      t.textContent = token;
      svg.appendChild(t);
    }
  }
}

function renderPlacementsOnBoard(placements, nodePositions) {
  if (!placements) return;
  drawPlacementForColor(nodePositions, "R1", "RED", placements.red1);
  drawPlacementForColor(nodePositions, "B1", "BLUE", placements.blue1);
  drawPlacementForColor(nodePositions, "O1", "ORANGE", placements.orange1);
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
    renderBoard(board, placements);
    renderPorts(board.port_resources || []);

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
  dataPathEl.textContent = `data: ${DATA_ROOT.pathname}`;
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
