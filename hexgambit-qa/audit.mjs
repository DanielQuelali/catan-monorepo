import fs from "node:fs";
import path from "node:path";
import { chromium, devices } from "playwright";

const BASE_URL = process.env.HEX_GAMBIT_URL || "http://127.0.0.1:8080";
const outDir = `/tmp/hexgambit-audit-${Date.now()}`;
fs.mkdirSync(outDir, { recursive: true });

function distancePointToSegment(px, py, ax, ay, bx, by) {
  const abx = bx - ax;
  const aby = by - ay;
  const apx = px - ax;
  const apy = py - ay;
  const ab2 = abx * abx + aby * aby;
  const t = ab2 === 0 ? 0 : Math.max(0, Math.min(1, (apx * abx + apy * aby) / ab2));
  const cx = ax + t * abx;
  const cy = ay + t * aby;
  const dx = px - cx;
  const dy = py - cy;
  return Math.hypot(dx, dy);
}

function parsePolygonPoints(pointsStr) {
  return pointsStr
    .trim()
    .split(/\s+/)
    .map((pair) => {
      const [x, y] = pair.split(",").map(Number);
      return { x, y };
    })
    .filter((p) => Number.isFinite(p.x) && Number.isFinite(p.y));
}

async function captureScenario(browser, name, contextOptions) {
  const context = await browser.newContext(contextOptions);
  const page = await context.newPage();

  await page.goto(BASE_URL, { waitUntil: "networkidle" });
  await page.waitForSelector("#app");
  await page.waitForTimeout(350);

  const introPath = path.join(outDir, `${name}-01-intro.png`);
  await page.screenshot({ path: introPath });

  const introMetrics = await page.evaluate(() => {
    const EPS = 1;
    const doc = document.documentElement;
    const body = document.body;
    const panelRect = document.getElementById("app")?.getBoundingClientRect() || null;
    return {
      stage: "intro",
      viewport: { w: window.innerWidth, h: window.innerHeight },
      scroll: {
        x: window.scrollX,
        y: window.scrollY,
        docW: doc.scrollWidth,
        docH: doc.scrollHeight,
        bodyW: body.scrollWidth,
        bodyH: body.scrollHeight,
      },
      panelRect,
      panelFits:
        panelRect !== null &&
        panelRect.left >= -EPS &&
        panelRect.top >= -EPS &&
        panelRect.right <= window.innerWidth + EPS &&
        panelRect.bottom <= window.innerHeight + EPS,
    };
  });

  await page.locator("#start-session").click();
  await page.waitForSelector(".board-svg");
  await page.waitForTimeout(350);

  const placementPath = path.join(outDir, `${name}-02-placement.png`);
  await page.screenshot({ path: placementPath });

  const placementMetrics = await page.evaluate(
    ({ distancePointToSegmentSrc, parsePolygonPointsSrc }) => {
      const distancePointToSegmentFn = new Function(`return (${distancePointToSegmentSrc});`)();
      const parsePolygonPointsFn = new Function(`return (${parsePolygonPointsSrc});`)();

      const EPS = 1;
      const doc = document.documentElement;
      const body = document.body;
      const svg = document.querySelector(".board-svg");
      const panelRect = document.getElementById("app")?.getBoundingClientRect() || null;
      const boardRect = document.querySelector(".board-wrap")?.getBoundingClientRect() || null;
      const portMarkers = [...document.querySelectorAll(".port-marker")];
      const spokes = [...document.querySelectorAll(".port-spoke")];
      const polygons = [...document.querySelectorAll(".hex-cell polygon")];
      const polySegments = polygons.flatMap((polygon) => {
        const points = parsePolygonPointsFn(polygon.getAttribute("points") || "");
        if (points.length < 2) return [];
        const segments = [];
        for (let i = 0; i < points.length; i += 1) {
          const a = points[i];
          const b = points[(i + 1) % points.length];
          segments.push([a.x, a.y, b.x, b.y]);
        }
        return segments;
      });

      const disconnectedSpokes = [];
      for (const [idx, line] of spokes.entries()) {
        const x2 = Number(line.getAttribute("x2"));
        const y2 = Number(line.getAttribute("y2"));
        let minToEdge = Infinity;
        for (const [ax, ay, bx, by] of polySegments) {
          minToEdge = Math.min(minToEdge, distancePointToSegmentFn(x2, y2, ax, ay, bx, by));
        }
        if (minToEdge > 2.5) {
          disconnectedSpokes.push({ spokeIndex: idx, nearestToLandEdgePx: Number(minToEdge.toFixed(2)) });
        }
      }

      return {
        stage: "placement",
        viewport: { w: window.innerWidth, h: window.innerHeight },
        scroll: {
          x: window.scrollX,
          y: window.scrollY,
          docW: doc.scrollWidth,
          docH: doc.scrollHeight,
          bodyW: body.scrollWidth,
          bodyH: body.scrollHeight,
        },
        panelRect,
        panelFits:
          panelRect !== null &&
          panelRect.left >= -EPS &&
          panelRect.top >= -EPS &&
          panelRect.right <= window.innerWidth + EPS &&
          panelRect.bottom <= window.innerHeight + EPS,
        boardRect,
        boardFits:
          boardRect !== null &&
          boardRect.left >= -EPS &&
          boardRect.top >= -EPS &&
          boardRect.right <= window.innerWidth + EPS &&
          boardRect.bottom <= window.innerHeight + EPS,
        board: {
          hasSvg: Boolean(svg),
          portMarkers: portMarkers.length,
          portSpokes: spokes.length,
          landHexes: polygons.length,
          disconnectedSpokes,
        },
      };
    },
    {
      distancePointToSegmentSrc: distancePointToSegment.toString(),
      parsePolygonPointsSrc: parsePolygonPoints.toString(),
    }
  );

  await context.close();
  return {
    screenshots: {
      intro: introPath,
      placement: placementPath,
    },
    metrics: {
      intro: introMetrics,
      placement: placementMetrics,
    },
  };
}

async function main() {
  const browser = await chromium.launch();
  try {
    const desktop = await captureScenario(browser, "desktop", { viewport: { width: 1366, height: 768 } });
    const mobile = await captureScenario(browser, "mobile", {
      ...devices["iPhone 12"],
      viewport: { width: 390, height: 844 },
    });

    const report = {
      baseUrl: BASE_URL,
      outDir,
      desktop,
      mobile,
    };
    const reportPath = path.join(outDir, "report.json");
    fs.writeFileSync(reportPath, JSON.stringify(report, null, 2));
    process.stdout.write(`${reportPath}\n`);
  } finally {
    await browser.close();
  }
}

main().catch((error) => {
  process.stderr.write(`${String(error?.stack || error)}\n`);
  process.exit(1);
});
