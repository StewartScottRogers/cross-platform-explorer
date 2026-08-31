---
id: CPE-1522
title: "Automated Gource commit-history visualization on the landing page"
type: Feature
status: Doing
priority: Low
component: CI
tags: [ready]
created: 2026-08-09
---
## Why (user request, 2026-08-09)
The user wants a dynamic, self-updating showcase of the project's git history — a Gource animation — embedded
on the public landing page, rendered by a scheduled GitHub Actions workflow.

## What (implemented — PR `feat: add automated gource history visualization`, open for user review)
- **`.github/workflows/gource-visualization.yml`** — runs weekly (`cron: 0 0 * * 0`) + `workflow_dispatch`;
  installs `gource`/`ffmpeg`/`xvfb`; renders the full history via `xvfb-run` → Gource `--output-ppm-stream` →
  FFmpeg (`libx264`, `yuv420p`, `crf 23`, `+faststart`) into `gource-history.mp4`; uploads it as a run
  artifact; publishes it to a dedicated **orphan `gource` branch** (single commit, **force-pushed**).
- **`README.md`** — a "📽 Watch the project evolve" section inserted **under the hero** (logo/title/badge/
  description/links preserved), embedding the video via `<video autoplay loop muted playsinline>` from the
  `gource` branch's raw URL, with a download-fallback link.

## Key design decision (deviates from the naive spec — logged)
The pasted spec committed the regenerated MP4 to **`main`** every week. **Rejected:** a 720p Gource video is
several–tens of MB and git keeps every version forever, so that would bloat `main`'s permanent history weekly —
directly against PURPOSE.md's small/lean guardrail. **Instead:** force-push a single-commit **orphan `gource`
branch** — one copy of the latest video, never accumulating, and `main` (what people clone) stays lean. Same
visible result. `[skip ci]` on the publish commit prevents re-triggering CI.

## To finish / verify (owed)
- **First render is manual:** after merge, run the workflow once via **Actions → Gource history visualization →
  Run workflow** (`workflow_dispatch`) to populate the `gource` branch — until then the README `<video>` src
  404s (shows the fallback text). Then confirm the README renders the animation on github.com.
- **Rendering caveat (honest):** GitHub's README `<video>` from a `raw.githubusercontent` URL renders inline in
  most modern browsers but autoplay/content-type handling can vary; if it doesn't autoplay inline, the download
  link still works. If the user prefers guaranteed inline playback, the alternative is serving the MP4 from the
  GitHub Pages site (`docs/`) and embedding in `docs/index.html` — noted as a follow-up option, not done here.
- Runner-usage: weekly only (never on push), per the spec's cost guard.

## Notes
Outward-facing (public landing page) → opened as a **review PR, not self-merged** (per the spec + the repo's
outward-facing-change caution). Disjoint from all in-flight sprint work (CI + README only).
