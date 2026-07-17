---
id: CPE-536
title: "Documents — the docs library + build-into-the-app bundling"
type: Feature
status: Open
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-534
sprint: SPR-07
estimate: 3-4h
created: 2026-07-16
---

## Summary
Foundation of Application → Documents ([[CPE-534]]): a **library** of professional markdown docs in the
repo, **bundled into the app at build time** (offline) via Vite's `import.meta.glob` (eager, raw), with
an ordered index. Write a genuinely useful **initial set** (not stubs).

## Acceptance Criteria
- [ ] A `src/docs/*.md` library, each doc with a title + order (frontmatter or filename prefix).
- [ ] A pure `docs.ts` loads them at build time (`import.meta.glob`), exposing an ordered
      `{ slug, title, order, content }[]` + a `search(query)` over title/body.
- [ ] An initial set covering: Overview, Getting Started, Explorer basics, AI Console, Agent Grid,
      Agent Board, Workbench, Repositories — real prose, not placeholders.
- [ ] The docs ship inside the built app (no network needed to read them).
- [ ] Tests for the index ordering + search.

## Notes
Wave 1 of [[CPE-534]]. Consumed by the viewer (CPE-537). Bundled at build via Vite glob keeps it simple
and offline while the default build stays lean.
