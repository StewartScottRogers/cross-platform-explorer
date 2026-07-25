---
id: CPE-536
title: "Documents — the docs library + build-into-the-app bundling"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-534
sprint: SPR-07
closed: 2026-07-16
estimate: 3-4h
created: 2026-07-16
---

## Summary
Foundation of Application → Documents ([[CPE-534]]): a **library** of professional markdown docs in the
repo, **bundled into the app at build time** (offline) via Vite's `import.meta.glob` (eager, raw), with
an ordered index. Write a genuinely useful **initial set** (not stubs).

## Acceptance Criteria
- [x] A `src/docs/*.md` library, each doc with a title + order (frontmatter or filename prefix).
- [x] A pure `docs.ts` loads them at build time (`import.meta.glob`), exposing an ordered
      `{ slug, title, order, content }[]` + a `search(query)` over title/body.
- [x] An initial set covering: Overview, Getting Started, Explorer basics, AI Console, Agent Grid,
      Agent Board, Workbench, Repositories — real prose, not placeholders.
- [x] The docs ship inside the built app (no network needed to read them).
- [x] Tests for the index ordering + search.

## Notes
Wave 1 of [[CPE-534]]. Consumed by the viewer (CPE-537). Bundled at build via Vite glob keeps it simple
and offline while the default build stays lean.

## Resolution
Added the docs library, bundled into the app at build time.

- **`src/docs/*.md`** — 8 real, professional docs (Overview, Getting Started, The Explorer, AI Console,
  Agent Grid, Agent Board, Workbench, Repositories), each with `title` + `order` frontmatter and genuine
  prose (not stubs).
- **`src/lib/docs.ts`** — pure `parseDoc` / `buildIndex` (ordered by order then title) / `searchDocs`
  (title+body, case-insensitive), plus `DOCS` built from **`import.meta.glob("../docs/*.md", { query:
  "?raw", eager: true })`** — the docs compile into the frontend bundle, so they work **offline** with no
  runtime fetch and the default build only grows by the markdown size.
- 5 tests incl. one asserting the **real bundled set** loads, is ordered, and every doc has content.

`npm run check` clean; 555 frontend tests pass. First ticket of SPR-07; consumed by the viewer (CPE-537).
Per the standing rule, this library will be updated/extended as future tickets land.

## Work Log
2026-07-16 — Picked up (SPR-07). Wrote 8 professional docs + docs.ts (glob-bundled index + search, pure/tested). npm check clean; 555 tests. All ACs met.
