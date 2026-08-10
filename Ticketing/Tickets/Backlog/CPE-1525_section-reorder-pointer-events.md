---
id: CPE-1525
title: "Sidebar section drag-reorder doesn't work in the real app (HTML5 DnD swallowed by Tauri dragDrop in WebView2) → rewrite with pointer events"
type: Bug
status: Backlog
priority: High
component: Frontend
tags: [ready]
epic: CPE-660
created: 2026-08-09
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
