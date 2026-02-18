# Hex Gambit Playwright Audit Guide

This guide explains how to run the local Playwright QA tool at:

- `hexgambit-qa/audit.mjs`

It is intended for AI dev agents validating Hex Gambit UI quality before handing off changes.

## Purpose

The audit script verifies:

1. The app renders in desktop and mobile.
2. No page scroll is triggered.
3. The panel and board fit fully inside the viewport.
4. The board SVG exists.
5. Ports are connected to coastline edges (spoke-to-land geometry check).
6. Screenshots are produced for visual review.

## Folder Rules

1. Tooling/code lives in repo root folder `hexgambit-qa/`.
2. Screenshots and `report.json` are written only to `/tmp/hexgambit-audit-<timestamp>/`.

## Prerequisites

1. Node.js + npm available.
2. Hex Gambit server running on `http://127.0.0.1:8080` (or custom URL via env var).
3. Playwright dependency installed in `hexgambit-qa/`.
4. Chromium installed for Playwright.

## One-Time Setup

From repo root:

```bash
cd hexgambit-qa
npm install
npx playwright install chromium
```

## Run Audit

1. Start app server from repo root:

```bash
./run-hex-gambit.sh
```

2. In another terminal, run audit:

```bash
cd hexgambit-qa
node audit.mjs
```

The command prints the generated report path, for example:

```text
/tmp/hexgambit-audit-1771237974506/report.json
```

## Custom Target URL

If running the app on another port/host:

```bash
cd hexgambit-qa
HEX_GAMBIT_URL=http://127.0.0.1:8090 node audit.mjs
```

## Output Artifacts

Each run writes:

1. `report.json`
2. `desktop-01-intro.png`
3. `desktop-02-placement.png`
4. `mobile-01-intro.png`
5. `mobile-02-placement.png`

All are saved in the same `/tmp/hexgambit-audit-<timestamp>/` directory.

## Pass/Fail Criteria

Read `report.json`. A valid run should satisfy all of:

1. `desktop.metrics.intro.panelFits === true`
2. `desktop.metrics.placement.panelFits === true`
3. `desktop.metrics.placement.boardFits === true`
4. `mobile.metrics.intro.panelFits === true`
5. `mobile.metrics.placement.panelFits === true`
6. `mobile.metrics.placement.boardFits === true`
7. `desktop.metrics.placement.board.disconnectedSpokes.length === 0`
8. `mobile.metrics.placement.board.disconnectedSpokes.length === 0`
9. `desktop.metrics.placement.board.portBadges === 9`
10. `mobile.metrics.placement.board.portBadges === 9`

If any check fails, treat the change as not production-ready.

## Fast Report Check

```bash
node -e "const r=require('/tmp/hexgambit-audit-<id>/report.json'); console.log(JSON.stringify({desktop:r.desktop.metrics.placement,mobile:r.mobile.metrics.placement},null,2));"
```

## Troubleshooting

1. `ECONNREFUSED` or blank screenshots:
- App server is not running or wrong URL. Start with `./run-hex-gambit.sh` and retry.

2. Playwright browser launch errors:
- Run `npx playwright install chromium` again.
- If sandbox blocks launch, rerun command with required elevated permissions in your environment.

3. Output path confusion:
- Use the exact printed path from `node audit.mjs`.
- Do not write screenshots inside repo; screenshots must stay under `/tmp`.

## Agent Workflow Recommendation

For UI changes affecting board/layout/ports:

1. Implement code change.
2. Run `node audit.mjs`.
3. Inspect both placement screenshots.
4. Enforce pass/fail criteria above.
5. Repeat until all checks pass.
