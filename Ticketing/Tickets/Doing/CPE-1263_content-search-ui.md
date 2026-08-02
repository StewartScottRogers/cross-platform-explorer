---
id: CPE-1263
title: "File-content search UI (query box + ranked results with snippets, wired to content_search)"
type: feature
component: frontend
priority: medium
status: Doing
tags: ready
created: 2026-08-02
epic: CPE-976
---

## Summary
Frontend for CPE-976 file-content search. Depends on CPE-1262 (the `content_index_build` + `content_search` commands).
Give the user a way to search files by what's INSIDE them and jump to a hit. Local embedder → works offline, no key.

## Build
- A content-search surface (reuse the existing search/panel conventions — prefer an inline instant control over a modal,
  per house style). Elements: a query input; a "build/refresh index for this folder" action with streamed progress
  (subscribe to `content_index_build`'s channel — show files-indexed count / a progress bar, per STREAMING.md); a ranked
  results list showing filename, relative path, a score indicator, and the snippet; click a result → navigate to its
  folder + select the file (reuse existing navigate/select).
- Empty/needs-build state: if no index exists for the folder, prompt to build it (don't show a raw error).
- Route all backend calls through `src/lib/invoke.ts` (busy-cursor wrapper) except the streamed build (use the streaming
  pattern like other bulk producers). Debounce the query; supersede in-flight searches by generation token.
- Follow UI conventions: pills/chips reflow (tick-tacks), light-theme palette vars only, menu/tab standards if any are added.

## Acceptance criteria
- Typing a query returns ranked content hits with snippets; clicking navigates+selects the file.
- Build-index shows live streamed progress and doesn't block the UI (busy cursor / progress).
- Component unit tests (jsdom, backend mocked) cover: query→results render, empty/needs-build state, navigate-on-click,
  debounce/generation-token supersede. `npm run check` clean.
- A `gui-smoke` render pin (seed a mocked/seeded result set via a test-mode hook if needed) OR at minimum a jsdom test
  pinning the render — match how sibling panels are pinned; note the MVD/burndown row if any human-visual residual remains.
- Docs (CPE-579): add/extend a content-search doc page in `src/docs/*.md` + its `sectionDocs.ts` entry; guard test green.

## Notes
Honest framing in UI copy: "search file contents" (embedder-pluggable), not overpromising "AI semantic". Visual Critic +
(if user-facing interaction feel) an attended pass may be needed — screenshot-judge first, minimize user involvement.
