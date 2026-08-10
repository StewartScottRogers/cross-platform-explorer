---
id: CPE-1530
title: "Drop Stack: client-side store + persistence (foundation)"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1489
parent: CPE-1489
created: 2026-08-09
---
## Context
CPE-1489's headline feature: a persistent "shelf" of files/folders accumulated across *different* folder
navigations (Path Finder's Drop Stack), later moved/copied all-at-once. Before any UI exists, the app needs
a single source-of-truth data model for the accumulated set — this is the **foundation** ticket every other
Drop Stack ticket (CPE-1531/1532/1533) imports from. It touches no existing files, so it carries zero
conflict risk and should be dispatched **first**.

## Scope
- A Drop Stack store: an ordered list of entries `{ path, addedFrom, addedAt }` (source path, the folder it
  was added from, timestamp).
- Operations: `add(entries)` (de-duped by path), `remove(path)`, `clear()`, plus a reactive/subscribable
  list for the (future) panel to render.
- Persistence: survive in-app navigation (trivially true for a module-level store) **and** survive app
  restart — persist via the existing `src/lib/settings.ts` settings.json-backed pattern (see its
  localStorage-migration comments), **not** raw `localStorage` calls scattered in a new file, so it stays
  consistent with how the app already persists user state.
- No UI in this ticket — pure store + persistence logic only.

## How
- New file `src/lib/dropStack.ts`: a Svelte `writable`-backed store (mirror the shape of
  `src/lib/transfers.ts` — pure reducer functions + a thin reactive tail) with `add`/`remove`/`clear` and
  a `dropStackEntries: Readable<DropStackEntry[]>` export.
- Persistence: read/write through `settings.ts`'s existing load/save plumbing (add a `dropStack` field to
  the settings shape) rather than inventing a second persistence mechanism.
- No new dependency.

## Verify
`npm run check` + `npx vitest run src/lib/dropStack.test.ts` — new suite covering add/de-dupe/remove/clear
as pure reducer functions (same style as `transfers.test.ts`), plus a settings-round-trip test (save then
reload restores the stack). Fully headless (no DOM, no OS, no network) — jsdom's `localStorage`/mocked
settings file is enough; no GUI verification needed to land it.

## Notes
**Conflict surface:** new files only — `src/lib/dropStack.ts`, `src/lib/dropStack.test.ts`, plus an
additive field in `src/lib/settings.ts`'s settings shape (small, additive — should not collide with
concurrent settings.ts edits, but flag to the Foreman if another in-flight ticket also touches
settings.ts). **Dispatch order: FIRST.** CPE-1531, CPE-1532, and CPE-1533 all import from this store and
must not start until it lands.
