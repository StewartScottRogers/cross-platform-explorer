---
id: CPE-1525
title: "Sidebar section drag-reorder doesn't work in the real app (HTML5 DnD swallowed by Tauri dragDrop in WebView2) → rewrite with pointer events"
type: Bug
status: Done
priority: High
component: Frontend
tags: [ready]
epic: CPE-660
created: 2026-08-09
closed: 2026-08-09
---
## Symptom (user, 2026-08-09, on installed v0.57.63)
The section-reorder grips (CPE-1520) show a grab cursor ("a giant hand") on hover but **dragging does nothing** —
sections can't be reordered in the real installed app. Screenshot confirms the grips render on every section
header (Tags/Explore/Quick Access/Drives/Network) but are inert.

## Root cause
`Sidebar.svelte`'s section reorder uses **HTML5 drag-and-drop** (`draggable="true"` + `dragstart/dragover/drop`;
handlers are correct, incl. `preventDefault` on dragover). But **Tauri's webview OS drag-drop handler is enabled**
(`dragDrop` is unset in `src-tauri/tauri.conf.json` → defaults to true) — required because the app consumes
file-drop-into-window (`App.svelte:5556 .onDragDropEvent`). On Windows/WebView2, an enabled Tauri drag-drop
handler **intercepts/swallows in-page HTML5 drag events**, so `draggable` DnD never fires a real drag. This is a
known Tauri limitation; it can't be fixed by `dragDrop:false` (that would kill the app's file-drop feature).
NOTE: the **pre-existing** favorites/quick-access/drive item drags in `Sidebar.svelte` also use `draggable="true"`
and are therefore **also likely non-functional** in the real app — see follow-up scope below.

## Fix — drive the reorder with POINTER events (not intercepted by WebView2)
- Replace the section-header HTML5 DnD (`draggable`, `on:dragstart/dragover/dragleave/drop/dragend`) with a
  **pointer-events** interaction on the **grip**: `pointerdown` → `setPointerCapture` + set `draggingSection`;
  `pointermove` → hit-test which section header is under the pointer (`document.elementFromPoint` climbing to the
  section id, or compare against cached header `getBoundingClientRect`s) → set `dragOverSection`/`dragOverBefore`
  + show the existing drop indicator; `pointerup` → `reorderSection(...)` + release capture + cleanup; also handle
  `pointercancel`/Escape.
- **Keep the pure reducer layer unchanged** (`sidebarOrder.ts` — `reorderSection`/`moveNextTo`/etc. + its unit
  tests are fine; only the DOM interaction changes).
- Preserve the drop indicator + grip affordance; keep it from starting a drag on a normal click (small movement
  threshold before it counts as a drag).
- Must NOT interfere with the file-drop-into-window handler or the file-list item drags.
- **Verification is real-app only** (WebView2) — jsdom can't exercise this; needs a build→install→run + the user's
  hands, OR a `gui-smoke` pointer-drag spec if feasible.

## Follow-up (note, don't necessarily do here)
If the pre-existing favorites/quick-access item drags are confirmed dead for the same reason, file a sibling
ticket to migrate those to pointer events too (or a shared helper). Consider also a **keyboard/menu fallback**
(right-click section header → "Move up / Move down") so reordering doesn't depend on drag at all — more reliable
and accessible.

## Notes
Interaction bug from CPE-1520; the jsdom tests couldn't catch it (they verify the reducer, not real DnD). High
priority — it's a visibly-broken shipped affordance the user is looking at right now.

## Work Log (2026-08-09)

**Root cause confirmed:** exactly as filed. `Sidebar.svelte`'s section headers were `draggable="true"` with
`on:dragstart/dragover/dragleave/drop/dragend`; the reducer logic was already correct, but Tauri's webview
drag-drop handler (enabled — `dragDrop` unset in `src-tauri/tauri.conf.json` → defaults `true`, required for
`App.svelte`'s file-drop-into-window `.onDragDropEvent`) swallows in-page HTML5 drag events on Windows/WebView2
before they fire, so `dragstart` never runs in the real app. `dragDrop:false` was correctly ruled out (kills
file-drop).

**Fix — rewrote the section-reorder DOM interaction to pointer events, in `Sidebar.svelte` only:**
- Removed `draggable="true"` and the `on:dragstart/dragover/dragleave/drop/dragend` listeners from all 9 section
  headers (agents, favorites, tags, smart, savedSearch, explore, places, drives, network). Each header now just
  carries `data-section-id="<id>"` for hit-testing — no drag listeners on the header itself.
- The **grip** (`.section-grip`, a `role="button" tabindex="0"` span) is now the pointer-events drag handle:
  - `pointerdown` → `setPointerCapture`, records the start point, sets `draggingSection`.
  - `pointermove` (fires on the grip because it holds capture, regardless of where the pointer physically is) →
    once movement exceeds a **4px threshold** it becomes a real drag; hit-tests via
    `document.elementFromPoint(x, y)` climbing `parentElement` until it finds `data-section-id` (capture doesn't
    affect `elementFromPoint`, which is a pure geometry query — confirmed this is safe), then sets
    `dragOverSection`/`dragOverBefore` from the header's `getBoundingClientRect()` midline, driving the exact
    same `drop-before`/`drop-after` indicator classes as before.
  - `pointerup` → if the threshold was crossed and a target is hovered, calls the **unchanged**
    `reorderSection(...)` from `sidebarOrder.ts`, releases capture, cleans up.
  - `pointercancel` and **Escape** (via `on:keydown`) cancel the in-progress drag without reordering.
  - A plain click (no movement past 4px) never calls `reorderSection` and never blocks the header's own
    collapse-toggle button, which is unaffected (separate `<button class="twisty">`, untouched).
- **Keyboard/a11y fallback added** (ticket's "if it doesn't bloat the PR" ask — it didn't): the grip is
  `tabindex="0"` with `role="button"`; **Arrow Up / Arrow Down** while focused call the pure `moveSection(id, -1|1)`
  helper that already existed in `sidebarOrder.ts` (unused until now) — moves the section one slot with no pointer
  drag at all. Added `:focus-visible` outline styling for the grip.
- `sidebarOrder.ts` (the pure reducer — `reorderSection`/`moveNextTo`/`moveSection`/etc.) is **completely
  unchanged**; only imported one more already-existing export (`moveSection`). Its 24 unit tests in
  `sidebarOrder.test.ts` are untouched and still green.
- CSS: moved `cursor: grab` off `.section-head` (no longer draggable itself) onto `.section-grip`, added
  `.section-grip.grabbing` for the active-drag cursor and `touch-action: none` so a touch/pen drag on the grip
  isn't hijacked into a scroll gesture.
- Did **not** touch the file-list item drags, `App.svelte`'s file-drop handler, or the pre-existing
  favorites/quick-access/drive-row `draggable` item drags flagged as a likely-also-broken follow-up in this
  ticket's Notes — that's out of scope here; worth a sibling ticket if confirmed.
- Updated `src/docs/03-explorer.md`'s sidebar-reorder bullet to mention the new keyboard fallback.

**Verification:**
- `npm run check` — 0 errors, 0 warnings.
- `npx vitest run` (full suite) — 233 files / 2641 tests passed, including `sidebarOrder.test.ts` (24 tests,
  reducer unchanged) and the existing `Sidebar.test.ts` / `Sidebar.hoverSameVolume.test.ts` component tests (27
  tests, unrelated file-drag paths untouched).
- **What's owed / NOT verified here:** jsdom has no real pointer-capture or hit-testing against actual layout, so
  the new drag interaction itself is **only compile/type/unit verified, not behaviorally verified**. This
  absolutely requires a build → install → run of the real app on Windows/WebView2 and a hands-on check that
  grabbing a section grip and dragging now actually reorders sections (confirming the pointer-events path isn't
  swallowed the way HTML5 DnD was) before this can be called fully done from the user's perspective.
