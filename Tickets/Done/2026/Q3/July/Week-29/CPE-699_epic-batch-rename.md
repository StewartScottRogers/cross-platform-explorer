---
id: CPE-699
title: "EPIC: Advanced batch rename"
type: Task
status: Done
resolution: Duplicate
closed: 2026-07-19
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
---

## Summary
Selecting many files and renaming them in one structured pass is a staple power-user feature of every
serious file manager (Explorer's Tab-rename, Total Commander's Multi-Rename Tool, Bulk Rename Utility,
`rename`/`mmv`). The app today only renames **one entry in-place**. Add a **batch rename** mode: pick a
set of entries, compose a rename *recipe* (find/replace, case, insert/remove, sequential numbering,
extension change…), see a **live old→new preview with collision detection**, and apply it as one
undoable operation.

## Goal
A user can select N files, open a Batch Rename panel, build a recipe from composable operations, watch
the preview update as they type, be warned before any name collision or invalid name, and apply — with
the whole batch renamed safely (no partial clobbering) and reversible. The rename logic is a **pure,
fully-tested engine**; the GUI is a thin shell over it. With batch-rename closed, the plain explorer is
unchanged and pays nothing (PURPOSE.md tiebreaker).

## Why this shape fits the codebase
The transformation is **pure string logic** — the same shape as `src/lib/search.ts`'s matcher (CPE-697):
`names[] + recipe → newNames[]`, with preview and collision detection derivable from that. That core is
100% unit-testable headlessly (vitest + cargo), so it can be built and verified with no GUI, while the
dialog and the actual filesystem apply are the attended, GUI-verified parts. This makes the epic
decompose cleanly into a safe headless core + attended shell.

## Rough scope (NOT fully decomposed — see child tickets)
- **Rename engine (pure).** A composable recipe of operations applied in order to each selected name:
  - find & replace (literal + regex, first/all, case-insensitive),
  - case transform (lower / UPPER / Title / Sentence),
  - insert (prefix / suffix / at position), remove (range / by pattern), trim + collapse whitespace,
  - **sequential numbering** (start, step, zero-pad width, position: prefix/suffix/at-N),
  - change / add / strip **extension**,
  - scope: apply to *name only* / *extension only* / *whole filename*.
  Output is `{ from, to, changed }[]`; the engine is stateless and pure.
- **Preview + validation.** Derive the old→new list; flag **collisions** (two results share a target, or a
  target hits an untouched sibling), **no-ops**, and **invalid names** (empty, illegal chars per-OS,
  reserved names like `CON`/`NUL` on Windows). No FS access — pure over the input name list.
- **Backend batch-apply command.** A collision-safe apply that orders renames to avoid intermediate
  clobbering (topological / via temp names for swaps/cycles), is atomic-ish (all-or-report), and skips
  unreadable entries like the rest of the FS layer. Cargo-tested on the ordering/cycle logic.
- **Batch Rename GUI.** A panel/dialog: operation rows (add/remove/reorder), the live preview table with
  collision highlighting, apply/cancel. Follows MENUS/TABS/tick-tacks conventions. **Attended GUI verify.**
- **Undo.** Apply returns the inverse map so the operation is reversible (fits any existing undo path, or
  a one-shot "Undo rename").
- **Docs (CPE-579).** A batch-rename section in `src/docs`, mapped in `sectionDocs.ts`.

## Open questions (resolve at activation → resolved below)
- Regex engine parity frontend (JS `RegExp`) vs backend (Rust `regex`)? → Preview is frontend-only; the
  backend just renames the exact `from→to` pairs the engine produced, so no regex reimplementation is
  needed backend-side. Backend validates the pairs (collision/illegal) defensively but does not recompute.
- Where does numbering count from — selection order, current sort order, or name order? → **current sort
  order of the selection** (what the user sees), passed as the input list order.

## Definition of Done (epic-level)
- Selecting N entries and applying a recipe renames all N per the previewed old→new mapping, with no
  partial clobbering and collisions blocked before apply.
- The rename engine + validation are pure and fully unit-tested (vitest); the backend apply/ordering is
  cargo-tested; `npm run check`, full JS suite, `cargo test`/clippy (both feature modes) green.
- The panel previews live with collision/no-op/invalid highlighting and matches UI conventions
  (GUI-verified).
- Apply is reversible (undo).
- Docs section shipped + mapped; plain explorer unchanged when the feature is unused.

## Notes
Filed at tier 4 of the Nightshift waterfall (2026-07-18): tiers 1–3 were exhausted of
safe-to-land-blind work — the open Backlog (dual-pane, remote-FS, virtualization) is all
attended/GUI/network-gated, and every epic was already activated. Batch rename is high-value, on-theme
for a general file explorer, and — crucially for unattended work — has a **large pure-logic core that
lands and verifies headlessly**, exactly like the CPE-697 matcher. Other tier-4 leads noted for later
loops: a **duplicate-file finder** (size-bucket + hash, backend-pure core) and **AI-assisted
organize/rename** (natural-language → rename-recipe suggestions via the AI console, dry-run testable).

## Work Log
2026-07-18 22:24 USMST — Filed AND activated in one Nightshift step (user asleep, best-guess autonomy per
nightshift-mode). Rationale above. Decomposed into the children below; building the headless engine child
(CPE-700) first this loop.

## Decisions (activated 2026-07-18, nightshift)
- **Build order:** the pure engine + validation first (CPE-700, headless) — the safe land-tonight slice —
  then the backend collision-safe apply command (CPE-701, mostly cargo-testable ordering logic), then the
  GUI panel (CPE-702, attended), then docs/undo folded into those.
- **Engine home:** frontend `src/lib/rename.ts` (mirrors `search.ts`), pure + tree-shakeable; the preview
  is frontend-derived, the backend applies exact pairs.
- **Numbering order:** the selection in its **current on-screen sort order**.
- **Collision policy:** block apply on any collision/invalid name; never auto-clobber. Swaps/cycles among
  the selected set are resolved via temp names by the backend apply.

## Child tickets
1. **CPE-700** — Batch-rename engine + validation (pure `src/lib/rename.ts`): composable ops, old→new
   preview, collision/no-op/invalid detection. Fully vitest-tested. Safe/headless. *(build first)*
2. **CPE-701** — Backend `rename_many` command: collision-safe ordering (temp names for swaps/cycles),
   all-or-report, skip-on-error; returns the inverse map for undo. Cargo-tested ordering/cycle logic.
   *(prereq: CPE-700's pair shape)*
3. **CPE-702** — Batch Rename panel (GUI): operation rows add/remove/reorder, live preview table with
   collision highlighting, apply/cancel + undo; docs section + `sectionDocs.ts` mapping. **Attended GUI.**

## Closed — Duplicate (2026-07-19)
This epic was filed **and** activated during the autonomous nightshift on the false premise that the app
"only renames one entry in-place." That was wrong: a full batch-rename feature already ships
(`BatchRenameDialog.svelte` + `batchRename.ts` + the `move_exact` backend, from CPE-424/426/427/481/630).
The frontend was never checked before filing. Its children — CPE-700 (engine), CPE-701 (`rename_many`),
CPE-702 (panel) — were all duplicates. Per the user's decision (2026-07-19), the duplicate code was
reverted; CPE-702 is closed Duplicate; CPE-700/701 annotated Reverted. Nothing to build. The only kept
nightshift change is the unrelated CPE-697 (brace-expansion glob).
