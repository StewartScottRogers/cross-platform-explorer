---
id: CPE-636
title: "EPIC: Universal drag-and-drop for files"
type: Task
status: Proposed
priority: Medium
component: Frontend
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal

Make **drag-and-drop a first-class, everywhere capability** for files and folders across the whole
application — not the partial, view-specific behaviour it is today. A user should be able to grab any
file (or multi-selection) in any view and drop it on any valid target: another folder in the file list,
a place/drive in the sidebar, a tab, another pane — **and** drag files *out* to other OS applications
and drop files *in* from the OS/desktop to import/copy them here. One consistent, discoverable
interaction model, wired through the transfer manager ([[CPE-613]]) so drops become tracked copy/move
operations rather than silent one-offs.

## Why

Drag-and-drop already exists but only in pockets: `FileList.svelte` makes rows `draggable` and handles
`dragstart`/`dragover`/`drop` onto folder rows (L175, L346–L352), `Sidebar.svelte` and
`SidebarNode.svelte` accept drops onto places/drives, and `BoardView.svelte` has its own card DnD. There
is no unified model: coverage is inconsistent between views, the rules (copy vs. move, valid targets,
visual affordance) are re-implemented per component, and there's no clear support for the two most
expected cases — **dragging files out** of the app into another application, and **dropping OS files in**
from the desktop/Explorer. Users reasonably expect a file explorer to let them drag anything anywhere;
the gaps make the app feel half-finished. Consolidating on one DnD layer that every view reuses, routed
through the shared transfer queue, closes those gaps and removes duplicated, drifting drag code.

## Rough scope (areas, not child tickets)

- **Audit + unify** the existing drag/drop code (`FileList`, `Sidebar`, `SidebarNode`, `BoardView`) into
  one shared model/helper: what is draggable, what is a valid drop target, and the copy-vs-move rule
  (currently Ctrl = copy, else move — L195–196). Every view reuses it instead of re-implementing.
- **Complete internal coverage:** drag any selection (respecting multi-select) from any file view onto
  any folder row, sidebar place/drive, breadcrumb, or tab; and — if dual-pane ([[CPE-617]]) lands —
  across panes.
- **Drag OUT to the OS** — start a native drag so files can be dropped into other applications
  (Tauri drag-out / `startDrag`), carrying real file paths.
- **Drop IN from the OS** — accept files dragged from the desktop/Explorer onto the window (Tauri
  `onDragDropEvent` / webview file-drop) and route them to a copy/import into the current folder.
- **Route through the transfer manager** ([[CPE-613]]) so every drag-move/drag-copy becomes a tracked
  operation with progress + conflict handling, not a fire-and-forget call.
- **Consistent affordances:** a uniform drop-target highlight, a drag image/badge showing item count,
  and clear copy vs. move cursor feedback, themed from variables (light/dark parity).

## Open questions (resolve at activation)

- **OS drag-out support in Tauri v2** — what's actually available (native `startDrag` plugin vs.
  HTML5 drag with file paths), and does it work uniformly on Windows/macOS/Linux? Investigate first;
  this gates the drag-out slice.
- **External drop semantics** — dropping OS files *into* the app: copy, move, or ask? And where — always
  the current folder, or the specific folder/place under the cursor?
- **Transfer-manager dependency** — does this epic require [[CPE-613]] to have landed, or do we ship a
  direct-invoke fallback and retrofit the queue?
- **Copy-vs-move default** — keep Ctrl=copy/else=move, or adopt the OS convention (same-volume = move,
  cross-volume = copy, modifier overrides)?
- **Scope of "all files"** — does this include drag/drop inside specialized views (Gallery, Agent Board,
  archive/zip contents, remote/cloud filesystems [[CPE-616]]), or explorer file lists + sidebar only for
  v1?
- **Multi-select drag** — confirm the drag payload always carries the full current selection, not just
  the grabbed row.

## Definition of Done

- One shared drag-and-drop layer is reused by every file view; the per-component ad-hoc handlers are
  gone or delegate to it.
- A user can drag any (multi-)selection onto any valid internal target (folder row, sidebar place/drive)
  and it performs a tracked copy/move via the transfer manager.
- Files can be dragged **out** of the app into another OS application.
- Files dragged **in** from the OS are copied/imported into the intended folder.
- Drop targets, drag image, and copy/move feedback are consistent and themed (light/dark) everywhere.
- No regression to existing keyboard copy/move or the single-pane explorer when DnD isn't used.
