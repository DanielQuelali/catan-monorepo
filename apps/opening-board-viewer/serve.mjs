#!/usr/bin/env node

import { createServer } from "node:http";
import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, "../..");

const mimeByExt = new Map([
  [".html", "text/html; charset=utf-8"],
  [".css", "text/css; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".json", "application/json; charset=utf-8"],
  [".svg", "image/svg+xml"],
  [".png", "image/png"],
  [".jpg", "image/jpeg"],
  [".jpeg", "image/jpeg"],
]);

function parseArgs(argv) {
  const out = { host: "127.0.0.1", port: 8091 };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--host" && argv[i + 1]) {
      out.host = argv[i + 1];
      i += 1;
      continue;
    }
    if (arg === "--port" && argv[i + 1]) {
      const value = Number(argv[i + 1]);
      if (Number.isFinite(value) && value > 0 && value < 65536) {
        out.port = value;
      }
      i += 1;
    }
  }
  return out;
}

function isWithinRoot(root, candidate) {
  const rel = path.relative(root, candidate);
  return rel === "" || (!rel.startsWith("..") && !path.isAbsolute(rel));
}

function contentTypeFor(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  return mimeByExt.get(ext) || "application/octet-stream";
}

function send(res, status, body, headers = {}) {
  const payload = typeof body === "string" ? Buffer.from(body, "utf-8") : body;
  res.writeHead(status, {
    "Content-Length": String(payload.length),
    "Cache-Control": "no-store",
    ...headers,
  });
  res.end(payload);
}

async function serveFile(res, absolutePath) {
  try {
    const stat = await fs.stat(absolutePath);
    if (!stat.isFile()) {
      send(res, 404, "Not found", { "Content-Type": "text/plain; charset=utf-8" });
      return;
    }
    const body = await fs.readFile(absolutePath);
    send(res, 200, body, { "Content-Type": contentTypeFor(absolutePath) });
  } catch (_) {
    send(res, 404, "Not found", { "Content-Type": "text/plain; charset=utf-8" });
  }
}

function resolvePathname(pathname) {
  if (pathname === "/") {
    return path.join(__dirname, "index.html");
  }
  const decoded = decodeURIComponent(pathname);
  const normalized = path.normalize(decoded).replace(/^(\.\.(\/|\\|$))+/, "");
  const absolute = path.resolve(repoRoot, `.${normalized}`);
  if (!isWithinRoot(repoRoot, absolute)) {
    return null;
  }
  return absolute;
}

function main() {
  const { host, port } = parseArgs(process.argv.slice(2));

  const server = createServer(async (req, res) => {
    const reqUrl = new URL(req.url || "/", "http://localhost");
    const absolutePath = resolvePathname(reqUrl.pathname);
    if (!absolutePath) {
      send(res, 400, "Bad request", { "Content-Type": "text/plain; charset=utf-8" });
      return;
    }
    await serveFile(res, absolutePath);
  });

  server.listen(port, host, () => {
    console.log(`Opening board viewer: http://${host}:${port}`);
    console.log(`Serving from repo root: ${repoRoot}`);
  });
}

main();
