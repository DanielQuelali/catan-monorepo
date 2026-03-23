# opening-board-viewer

Static diagnostics viewer for opening-state board fixtures and holdout analysis CSVs.

## Run locally

From repo root:

```bash
node apps/opening-board-viewer/serve.mjs --host 127.0.0.1 --port 8091
```

Open:

- `http://127.0.0.1:8091/apps/opening-board-viewer/`

## Data sources

Default roots:

- board/state fixtures: `/data/opening_states/`
- holdout analysis artifacts: `/runtime-data/opening_states/`

Optional query params:

- `data=<url>` overrides fixture root.
- `analysis=<url>` overrides holdout-analysis root.

Example:

`http://127.0.0.1:8091/apps/opening-board-viewer/?data=/data/opening_states/&analysis=/runtime-data/opening_states/`

## Current implementation shape

- Single-file app entrypoint: `apps/opening-board-viewer/app.js`.
- Local static server: `apps/opening-board-viewer/serve.mjs`.
- Poll-based reload behavior for fixture/analysis changes.
