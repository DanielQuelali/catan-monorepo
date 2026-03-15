---
name: hexgambit-qa-screenshot
description: Capture fresh Hex Gambit UI screenshots using the local `hexgambit-qa` Playwright audit and provide direct image paths for inspection. Use when validating visual regressions (tokens, colors, pips, layout, spacing), confirming a UI change before/after edits, or when a user asks to "take a screenshot and look at it" for Hex Gambit.
---

# HexGambit QA Screenshot

Run the existing `hexgambit-qa/audit.mjs` flow against a temporary local server and extract concrete screenshot paths for immediate inspection.

## Quick Run

From repo root:

```bash
bash .codex/skills/hexgambit-qa-screenshot/scripts/capture_hexgambit_screenshots.sh
```

The script prints JSON with:
- `report_path`
- `desktop_intro`
- `desktop_placement`
- `mobile_intro`
- `mobile_placement`

## Workflow

1. Run the script above to produce fresh screenshots.
2. Open the desired image path with the image viewer tool.
3. Inspect tokens, colors, pips, and geometry directly in the screenshot before proposing edits.

## Options

Use a running server:

```bash
bash .codex/skills/hexgambit-qa-screenshot/scripts/capture_hexgambit_screenshots.sh --base-url http://127.0.0.1:8080
```

Use a custom temporary port:

```bash
bash .codex/skills/hexgambit-qa-screenshot/scripts/capture_hexgambit_screenshots.sh --port 18082
```

## Troubleshooting

If Playwright browser launch fails due sandbox restrictions, rerun the capture command with elevated permissions.

### scripts/
- `scripts/capture_hexgambit_screenshots.sh`: Starts a temporary Hex Gambit server (unless `--base-url` is provided), runs `hexgambit-qa/audit.mjs`, and prints screenshot paths.
