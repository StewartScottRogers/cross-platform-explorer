---
id: CPE-636
title: Frontend tag service
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
estimate: 1h
created: 2026-07-18
closed: 2026-07-18
epic: CPE-614
---

# CPE-636 — Frontend tag service

## Summary

Add an isolated frontend module (`src/lib/tags.ts`) that mirrors the CPE-635 backend tag store
(`load_tags` / `set_tags` / `tag_counts`) as a reactive Svelte store, following the repo's
store/service convention (a `writable` store + a thin `invoke` tail + pure, unit-tested helpers).
No existing component is touched — this is a self-contained new module other Agent-Tags work
(epic CPE-614) will build on.

## Acceptance Criteria

- [x] `src/lib/tags.ts` exports the `TagEntry` / `TagStore` types.
- [x] A `writable<TagStore>({})` exposed as a readable `tags` store.
- [x] `initTags()` loads the store once (idempotent) via `load_tags`.
- [x] `setEntryTags(path, tags, label)` calls `set_tags` and updates the store from the return.
- [x] `tagCounts()` returns `[string, number][]` via `tag_counts`.
- [x] Pure helpers `entryFor`, `hasTag`, `allTags`, `labelColor` + a `LABEL_COLORS` palette.
- [x] Module is DOM-free and side-effect-free except the store/invoke tail; imports `invoke`
      from `./invoke` (the busy-tracking wrapper).
- [x] `src/lib/tags.test.ts` unit-tests the pure helpers.
- [x] `npm run check` clean (0 errors / 0 warnings).
- [x] `npx vitest run src/lib/tags.test.ts` and the full suite are green.

## Resolution

Created `src/lib/tags.ts`:

- Types: `TagEntry { tags: string[]; label: string }`, `TagStore = Record<string, TagEntry>`.
- Reactive `tags` (readable over a private `writable<TagStore>({})`).
- Store tail: `initTags()` (idempotent load), `setEntryTags(path, tags, label)` (set + refresh),
  `tagCounts()`.
- Pure helpers: `entryFor(store, path)`, `hasTag(store, path, tag)`, `allTags(store)` (distinct,
  sorted), `LABEL_COLORS` (none→"" plus red/orange/yellow/green/blue/purple/grey hexes), and
  `labelColor(label)` (hex or "").

Added `src/lib/tags.test.ts` covering `entryFor` (present + missing), `hasTag`, `allTags`
(dedupe + sort), and `labelColor` (known + unknown/empty → "").

## Work Log

- Studied `src/lib/transfers.ts` + `src/lib/settings.ts` for the store/service convention and
  `src/lib/invoke.ts` for the busy-tracking wrapper; confirmed the CPE-635 backend command
  signatures in `src-tauri/src/lib.rs` (`load_tags`, `set_tags`, `tag_counts`).
- Wrote `src/lib/tags.ts` (DOM-free, pure helpers + thin invoke tail) and `src/lib/tags.test.ts`.
- Verified: `npm run check` → 0 errors / 0 warnings; `npx vitest run src/lib/tags.test.ts` →
  4/4 pass; `npx vitest run` → 69 files / 654 tests pass.
