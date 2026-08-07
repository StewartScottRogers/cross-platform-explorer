---
question: "After CPE-1370–1374, what headless-verifiable dual-pane work is left?"
date: 2026-08-06
status: current
tags: [frontier, headless, dual-pane, commander, cpe-617, pane-b, parity, app-svelte, vitest]
---

# The pane-B parity vein (dual-pane commander, epic CPE-617)

**Finding:** Diffing the pane-A vs pane-B `<ExplorerPane>` instantiation blocks in `src/App.svelte`
(pane A ~L4945–5014; pane B ~L5022–5048) turns up ~8 more gaps of the same shape that produced
CPE-1370–1374: props/events wired for pane A but silently absent for pane B. Confirmed against the
epic's own closing note ("pane-B DnD/context-menu … future refinements"). All are unit-testable via
vitest/jsdom (precedents: `App.filterReset.test.ts`, `App.contextmenu.test.ts`, `selection.test.ts`).

## The gaps (→ tickets filed CPE-1376..1381)
1. search + file-type filter props not passed to pane B (always shows unfiltered) — **CPE-1376**
2. right-click context menu inert in pane B (no rowContext/driveContext/contextEmpty/homeItemContext) — **CPE-1377**
3. inline rename can't complete in pane B (renamingPath/renameValue/commitRename unbound) — **CPE-1377**
4. "show folder sizes" never populates in pane B (showFolderSizes/folderSizes/needSizes) — **CPE-1376**
5. cut-highlight (Ctrl+X dim) never shows in pane B (cutPaths not passed) — **CPE-1376**
6. color-tag filter unusable in pane B (selectedTag/filterTag) — **CPE-1376**
7. custom metadata column widths/picker unusable in pane B — **CPE-1378**
8. Home-screen actions inert in pane B (inHome + unpin/unfavorite/removeRecent/network/loadShared) — **CPE-1378**
9. `dnd.ts` `norm()` self-descendant guard is case-SENSITIVE — folder-into-itself drop can slip on
   Windows/macOS (C:\Foo onto C:\FOO\sub). Pure fn, trivial test. **PARALLEL-SAFE** — **CPE-1379**
10. clipboard ops (doCopy/doCut/doPaste, App.svelte ~L2621–2867) — audit for the same wrong-pane bug as
    CPE-1370; route through activePane if not already. **CPE-1380** (verify vs CPE-1370 scope first)

## Conflict surface — CRITICAL for the Foreman
Items 1–8 (CPE-1371 DnD + CPE-1376/1377/1378) all edit the **same ~30-line pane-B block** in App.svelte
→ they **CANNOT run in parallel**; serialize (one worker owns the block, branch each off the prior merge)
or bundle by theme. Item 9 (dnd.ts) and item 10 (different App.svelte functions) are the only
parallel-safe slices — BUT item 9 collides with CPE-1372 which also edits dnd.ts, so it waits for
CPE-1372 to merge.

## What's genuinely tapped (do not re-scout)
Every `src/lib/*.ts` without a `.test.ts` sibling is generated/type/constant (bindings.gen.ts, types.ts,
diagnostics.ts, metaColumnCatalog.ts, newFileTypes.ts, templateVars.ts) — no logic gap. No TODO/FIXME/HACK
in src/lib or App.svelte. Epics CPE-711 (advanced selection) and CPE-978 (smart folders) are Done/verified.
Pure-module + file-type-signature veins confirmed tapped (see [[file-type-signature-vein-tapped-2026-08-05]]).

## User-gated (NOT headless — skip)
GUI visual/interaction-feel (build→deploy→run), HEIC native decode (licensing), AI features (API keys),
SFTP/cloud (creds), macOS-only paths (a Mac), signing cert, Docker net-E2E.
