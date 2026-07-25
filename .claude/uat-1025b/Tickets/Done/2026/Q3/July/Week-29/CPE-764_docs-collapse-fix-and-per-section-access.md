---
id: CPE-764
title: Documents follow-ups — fix section collapse + per-section doc access everywhere (incl. sidecar)
type: feature
component: Frontend
tags: ready
created: 2026-07-19
closed: 2026-07-19
status: Done
priority: high
estimate: 3-4h
---

## Summary
Two pieces of user feedback on the CPE-763 Documents upgrade:

1. **Category sections wouldn't collapse.** Clicking a category header did nothing.
2. **Docs were only reachable from the main menu.** The user expected a Documents button/menu at *every*
   place a documented section lives, each jumping straight to that section's page.

## 1 · Collapse bug (Svelte reactivity)
The TOC called `isExpanded(g.name)` — a function whose body reads `collapsed`/`searching`. Svelte only
re-renders a markup expression when a variable *named in the markup* changes, so `collapsed` was never a
tracked dependency: toggling it updated state but never re-rendered the group. Fixed by computing
`{@const expanded = searching || !collapsed[g.name]}` in the each-block (references the state directly, so
Svelte tracks it) and driving the header/chevron/list off `expanded`. Removed the now-unused `isExpanded`.

## 2 · Per-section access from everywhere
Design chosen with the user: **labeled header buttons + right-click entry + Command Palette entries**, and
**include the sidecar**.

- **Labeled header button.** `HelpButton` now shows a bordered "📖 Docs" chip (not a bare icon) so it's
  discoverable; the main toolbar's help button is likewise labeled ("Docs"). Present on the Explorer
  toolbar and the Disk-usage / Workbench / Agent-board headers, each opening its own page.
- **Right-click → "Documents for this view"** added to both the item and empty context menus (new
  `help-docs` action → `openDocs(currentSection())`).
- **Command Palette** gains a **Documents** group with a per-section entry for every documented section
  (`Docs: Overview / Explorer / Disk usage / Workbench / Agent Board / AI Console / Agent Grid /
  Repositories / Swarms`) → jump to any page from Ctrl+Shift+P.
- **Sidecar (AI Console).** The launcher has its own built-in help panel; made it **jump-to-anchor**
  (`openHelp(anchor)`) and added per-area **"?"** buttons in the **Grid** and **Swarm** areas that open
  the panel to their section. Added the previously-missing **Swarms** help section. Needs the sidecar
  host rebuilt (launcher is bundled into the sidecar).

## Files
- `src/lib/components/DocsView.svelte` — collapse fix (`{@const expanded}`), remove `isExpanded`.
- `src/lib/components/HelpButton.svelte` — labeled "Docs" chip variant (`label` prop).
- `src/lib/components/NavToolbar.svelte` + `src/app.css` — labeled toolbar Docs button.
- `src/lib/components/ContextMenu.svelte` — "Documents for this view" in both branches.
- `src/App.svelte` — `runAction` `help-docs` case; `DOC_SECTIONS` + per-section palette entries.
- `sidecar/ai-console/src/launcher.html` — `openHelp(anchor)`, Grid/Swarm "?" buttons, Swarms help
  section, `.area-help` style.

## Acceptance
- [x] Category sections collapse/expand on header click
- [x] Labeled "Docs" buttons in the Explorer toolbar + Disk-usage/Workbench/Agent-board headers
- [x] Right-click "Documents for this view" (item + empty menus)
- [x] Command Palette per-section "Docs: …" entries for all 9 sections
- [x] Sidecar: per-area "?" in Grid + Swarm open the help panel to their section; Swarms section added
- [x] `npm run check` 0/0; `npx vitest run` green (742); launcher jsdom harness green (62)

## Resolution (closed 2026-07-19)
Fixed the collapse reactivity bug and added per-section documentation access across every surface: labeled
header buttons, a right-click entry, per-section Command Palette entries, and — in the sidecar — anchor-jump
help with per-area "?" buttons plus a new Swarms help section. Verified green (svelte-check 0/0, 742 FE
tests, 62 launcher tests). The collapse fix landed first (commit on `main`); this closes the round.

## Work Log
- 2026-07-19 — Collapse bug reported; root-caused to Svelte not tracking a function-internal dependency.
  Fixed with `{@const}`, committed + merged to main.
- 2026-07-19 — User: docs only reachable from main menu. Chose labeled buttons + context menu + palette +
  sidecar. Implemented all; check 0/0, 742 + 62 tests pass. Closed and merged to main.
