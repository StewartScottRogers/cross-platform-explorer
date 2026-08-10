---
id: CPE-1532
title: "Drop Stack: dockable panel — list, remove, clear-all"
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
The Drop Stack needs somewhere to actually be *seen* — a small, optional, dockable panel listing what's
been added so far (source path per item), with per-item remove and a clear-all. This ticket builds the
panel shell and its list rendering only; the Move-all/Copy-all actions land in CPE-1533 (sequenced after
this one, since it adds buttons to the same component file).

## Scope
- New `DropStackPanel.svelte`: renders the current stack (from CPE-1530's store) as a **reflowing pill
  row** — [[tick-tacks-reflow]]: the container wraps pills onto more rows and grows height
  (`flex-wrap:wrap`), each pill stays one line, doesn't shrink, and gets `max-width` + ellipsis for long
  paths. Each pill shows the item name (title/tooltip = full source path) + a small per-item remove (×).
- A "Clear all" control.
- A toggle to show/hide the panel (e.g. a small toolbar/sidebar affordance) — off by default, opt-in, so
  the plain explorer is unaffected when unused (PURPOSE.md tiebreaker: zero cost when off).
- Empty state: a plain "Drop Stack is empty — right-click a file → Add to Drop Stack" hint, no dead
  chrome.
- Light-theme only (per [[app-is-light-theme-only]]) — theme vars, no hard-coded colours; visible border
  if it renders as an overlay/floating panel (per [[dialogs-need-visible-border]]) — if instead it's an
  inline docked panel (not an overlay), the border rule doesn't apply; pick the simpler docked-panel shape
  unless there's a reason to float it.
- Docs: this is a new user-facing surface — ship `src/docs/NN-drop-stack.md` (next available number after
  `30-structured-previews.md`) and add its `Section` entry to `src/lib/sectionDocs.ts` (CPE-579 — the
  guard test `src/lib/sectionDocs.test.ts` fails CI without it).

## How
- New file `src/lib/components/DropStackPanel.svelte`, subscribing to CPE-1530's `dropStackEntries` store
  and calling its `remove`/`clear`.
- Mount point: a small, additive toggle + `<DropStackPanel>` instantiation in `src/App.svelte`'s layout
  (mirror how `TransferPanel.svelte` is mounted, for consistency) — keep this integration point minimal.
- No new dependency.

## Verify
`npm run check` + `npx vitest run src/lib/components/DropStackPanel.test.ts` — new suite (jsdom, Svelte
Testing Library pattern already used by e.g. `HomeView.test.ts`) covering: renders current stack entries as
pills, remove-one calls store `remove`, clear-all calls store `clear`, empty state renders the hint, pills
don't overflow their background for a long path (reflow contract). Fully headless; no GUI verification
required to land it, though it's a good gui-smoke screenshot candidate later (light-theme panel rendering).

## Notes
**Conflict surface:** new files `src/lib/components/DropStackPanel.svelte`,
`src/lib/components/DropStackPanel.test.ts`, `src/docs/NN-drop-stack.md`, plus additive entries in
`src/lib/sectionDocs.ts` and a small mount point in `src/App.svelte` (toggle + one component
instantiation — keep this touch minimal to avoid colliding with CPE-1531's App.svelte edits; if both are
in flight, the Foreman should serialize the App.svelte hunks or land one first). **Dispatch order: after
CPE-1530.** Independent of CPE-1531 (different files) — can run in parallel with it. CPE-1533 depends on
this ticket landing first (it adds buttons to `DropStackPanel.svelte`).
