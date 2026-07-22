---
id: CPE-727
title: "EPIC: Edit diff peek — see what each agent write changed"
type: Task
status: Done
priority: High
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-18
closed: 2026-07-19
---

## Resolution
Closed 2026-07-19 — all four children delivered and merged to `main` (aa12b17). Agent Watch can now show
*what each agent write changed*, end to end: **CPE-743** (host shadow-content store pairs each write with its
cached "before", emitted on `ai-console://fs-diff`) → **CPE-744** (frontend per-path diff store) → **CPE-745**
(inline diff peek on hover of a write-touched timeline entry, with a "+a −d" summary) → **CPE-746** (full
side-by-side before/after modal). DoD met: hovering a write shows the exact hunks, click opens side-by-side;
snapshots are bounded (LRU by bytes/entries) and text-only; off-means-off (feature-gated, freed on stop).
Follow-up (documented, not blocking): the FileList-row peek was deferred (virtualized list — needs the
CPE-707 column work + a GUI pass); the timeline is the review surface today.

## Goal
Capture a before/after snapshot per mutation and render an inline, scrubbing-friendly diff: hover a touched
row or timeline entry to see the exact hunks the agent wrote, click to open a full side-by-side.

## Why
Every `modified`/`created` event annotates a row but shows nothing about *what* changed. The single most
useful thing about a write is its content — this is squarely on Agent Watch's visibility tiebreaker.

## Rough scope (areas, not child tickets)
- Content snapshotting on watch events (bounded, size-capped) to a diff store.
- Hunk computation reusing the existing diff renderer.
- Inline diff peek on touched rows + timeline entries; full side-by-side on click.
- Eviction/retention policy for snapshots.

## Open questions (resolve at activation)
- Snapshot size caps and which files to snapshot (skip binaries/huge files?).
- Storage location and retention of before/after content.
- Interaction with checkpoint/rollback ([[CPE-732]]) which also needs snapshots.

## Definition of Done
- Hovering a touched row/timeline entry shows the exact hunks written; click opens side-by-side.
- Snapshots are bounded and evicted per policy; large/binary files degrade gracefully.
- With Agent Watch off, no snapshotting occurs.

## Decisions (activated 2026-07-19 — best-guess logged; user directed autonomous, no questions)

Research confirmed this is **genuinely unbuilt**: there is no agent-write snapshot/diff module in `src/`,
`src-tauri/`, or the sidecar. CPE-405 shipped read *visibility* but nothing captures write *content*. The
existing `diff.ts` already provides `inlineDiff(oldText, newText)` and `parseDiff`, so the frontend can
render before/after without new diff machinery — the missing half is capturing the "before".

- **Where "before" comes from:** the FS watcher's `modified` fires *after* the write, so the host must keep a
  **shadow-content store** — cache last-seen text under the watched tree and pair new-vs-cached on each
  mutation (CPE-743). This is the foundation and the only backend piece.
- **Diff computed frontend-side:** host ships `{path, before, after}`; the frontend diffs via `diff.ts`
  (keeps the host dumb, reuses tested code).
- **Bounded + text-only:** skip binary/non-UTF8 and files over a per-file cap (~1 MiB); bound the store by
  bytes + count with LRU eviction. In-memory, per-session (no on-disk store) — keeps the delete-test clean.
- **Channel:** a **separate** `ai-console://fs-diff` event, so `fs-activity` stays lean.
- **Relationship to CPE-732 (Checkpoint & Rollback):** the shadow store could later back rollback snapshots;
  kept amenable but out of scope here.

## Child tickets
1. **CPE-743** — Host shadow-content store + before/after capture (bounded, text-only, LRU); emits
   `{path, before, after}` on a new channel. **Backend, foundation, cargo-tested.** *(needs-decision)*
2. **CPE-744** — Frontend per-path diff store ingesting the before/after records; hunks via `diff.ts`.
   *(prereq: 743)*
3. **CPE-745** — Inline diff peek on hover of a write-touched row / timeline entry. *(prereq: 744)*
4. **CPE-746** — Full side-by-side before/after view on click. *(prereq: 744; pairs with 745)*

## Work Log
2026-07-19 — Activated (autonomous). Verified genuinely unbuilt (no write-snapshot module; `diff.ts`
primitives reusable). Decomposed into 4 children CPE-743–746: one backend foundation (shadow-content store)
+ three frontend slices (diff store, inline peek, side-by-side). Set status In Progress.
