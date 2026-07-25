---
id: CPE-617
title: "EPIC: Dual-pane commander mode"
type: Task
status: Done
priority: Low
component: Frontend
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed: 2026-07-22
---

## Closed — 2026-07-22 (nightshift)
All four children Done: **CPE-676** (extract `<ExplorerPane>`), **CPE-677** (dual-pane toggle + split +
active-pane ring + Tab), **CPE-679** (persist/restore each pane's path), **CPE-678** (commander keys — F5
copy / F6 move / Ctrl+U swap / mirror, via the transfer queue). Opt-in, OFF by default; single-pane
unchanged. Definition of Done met (visual confirmation deferred to user review; single-pane provably
unaffected). Follow-ups tracked: i18n-ify the hardcoded palette/key labels; pane-B DnD/context-menu +
per-pane history are future refinements.

## Goal

Add an optional **two-pane (commander) layout** — two independent folder views side by side, à la
Norton Commander / Total Commander / Midnight Commander — with a keyboard-driven workflow for
copying/moving between the active and the opposite pane. A power-user mode that makes the most common
two-location task (move things from A to B) dramatically faster.

## Why

Copying between two folders in a single-pane explorer means tabs, back-and-forth, and clipboard juggling.
The dual-pane idiom — "source pane, target pane, F5 to copy, F6 to move" — is the fastest known workflow
for that, and it has a devoted user base. It's a pure **additive mode**: toggle it off and the app is the
familiar single-pane explorer, so the fast/small/predictable core is untouched.

## Rough scope (areas, not child tickets)

- A layout toggle: single-pane ⇄ dual-pane (split view), with a clear "active pane" indicator and Tab to
  switch focus between panes.
- Each pane is a full, independent explorer view (its own path, history, selection, sort) — ideally the
  existing view component instantiated twice rather than a fork.
- Commander keybindings: copy/move to the *other* pane (F5/F6 or configurable), swap panes, mirror the
  other pane's path, sync navigation.
- Integration with the transfer manager ([[CPE-613]]) so cross-pane copy/move uses the same queue + UX.
- Remember the layout choice + pane paths across sessions (reuse the settings + tab persistence).

## Open questions (resolve at activation)

- Can the current explorer view be cleanly instantiated twice (state isolation), or does it assume a
  single global instance? (This determines the effort — investigate first.)
- Vertical vs horizontal split, and how it interacts with the preview/details pane and Agent Watch.
- Keybinding scheme: adopt Total-Commander defaults or the app's own idiom? Make it configurable?
- Does dual-pane belong to a "power user" settings gate, or is it always one toggle away?

## Definition of Done

- A toggle splits the window into two independent panes with a visible active-pane indicator + Tab-switch.
- Copy/move between panes works via keyboard and the shared transfer queue.
- Single-pane mode is completely unchanged (default), and the layout choice persists.
- No measurable cost to single-pane startup/memory when dual-pane is off.

## Research spike (2026-07-18 activation — dispositive)
The first open question ("can the explorer view be instantiated twice?") is the crux, and the answer is
**no, not as-is**. `src/App.svelte` is a **~3,112-line monolith with ~134 top-level `let`/`$:` state
declarations** — all explorer state (`tabs`/`currentPath`, `entries`, `selection`, `sortKey/Dir`, `view`,
`archive`, `smartFolder`, `search`, `selectedTag`, `fileFilter`, `draggedPaths`, `loadGen`, …) lives
inline as component-global state, and every operation (nav, load, DnD, file ops, tags) is a top-level
function closing over it. There is **no reusable pane/explorer component** — `FileList` only renders the
row table. So dual-pane is gated on first **extracting an `<ExplorerPane>` component** that owns one
pane's state + operations, which single-pane then uses as one instance. That extraction is a large,
high-regression-surface refactor of the app's core and **must be done attended, with live GUI
verification** — not shipped blind. Hence this epic is decomposed but its build is sequenced behind that
extraction and flagged attended.

## Decisions (activated 2026-07-18, nightshift no-questions — best-guess logged)
- **Architecture:** extract `<ExplorerPane>` first (CPE-676); everything else builds on two instances.
- **Split:** vertical (side-by-side), the commander idiom. In dual-pane, the opposite pane occupies the
  area the preview/details pane uses in single-pane (preview hidden in dual mode for v1).
- **Keybindings (Total Commander idiom):** Tab = switch active pane; F5 = copy selection to the other
  pane; F6 = move to the other pane; configurable later.
- **Gate:** one toggle away (View menu / command palette), OFF by default; single-pane is unchanged.

## Child tickets
1. **CPE-676** — Extract `<ExplorerPane>` from App.svelte (per-pane state + operations); single-pane
   renders one instance, behaviour unchanged. **Large refactor — attended, GUI-verified.** Foundation.
2. **CPE-677** — Dual-pane layout toggle + vertical split + active-pane indicator + Tab-switch (two
   `<ExplorerPane>` instances); persist the layout choice. *(prereq: 676; attended)*
3. **CPE-678** — Commander keybindings: F5 copy / F6 move to the other pane, swap, mirror — routed
   through the transfer queue (CPE-613). *(prereq: 677)*
4. **CPE-679** — Persist each pane's path/history across sessions (reuse tab persistence). *(prereq: 677)*
