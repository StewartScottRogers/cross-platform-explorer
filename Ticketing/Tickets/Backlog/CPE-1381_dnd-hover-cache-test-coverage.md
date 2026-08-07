---
id: CPE-1381
title: "Test coverage: dnd hover sameVolume cache (dedup + race guard + reset) in FileList/Sidebar"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-661
created: 2026-08-06
---

## Problem (CPE-1372 / PR #656 reviewer fast-follow)

CPE-1372 (PR #656) added a per-hovered-destination `sameVolume` cache to `FileList.svelte`'s and
`Sidebar.svelte`'s `onDragOver` — one `commands.sameVolume()` IPC call per distinct hovered target, with a
stale/late-resolve guard and reset on drag end. The pure `hoverEffect` fn is well tested (`dnd.test.ts`,
10/10), but this actually-risky async cache/race code in the two components has **zero** test coverage.
`FileList.test.ts`/`Sidebar.test.ts` have no `dragover`/`sameVolume` tests.

## Fix direction

Add tests (the mocking pattern already exists in `FileList.archiveDragOut.test.ts` — `vi.mock("../bindings.gen", ...)`)
asserting: (a) exactly one `commands.sameVolume` call per distinct hovered dest, not per `dragover` tick;
(b) the cache resets on `dragend`; (c) a late-resolving promise for a since-abandoned hover does NOT override
the current hover's `dropEffect`. Touches `src/lib/components/FileList.test.ts` + `Sidebar.test.ts` only (new
test files / cases) — parallel-safe against the App.svelte pane-B chain. Best done AFTER #656 merges.
