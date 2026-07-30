---
id: CPE-1142
title: "Rules-based auto-organize: wire the built organize engine into a propose → checkpoint → apply feature"
type: feature
component: Multiple
priority: high
status: Backlog
tags: ready
created: 2026-07-29
epic: CPE-979
---

## Summary
Epic CPE-979's **rules-based** slice (the part that needs NO AI/model — the epic splits "rules-driven" from
the model-gated "AI-assisted" mode). The engine in `crates/server/src/organize.rs` is fully built + tested but
wired to zero commands:
- `plan_organize(entries: &[OrganizeEntry], rule: OrganizeRule) -> Vec<MoveProposal>` — one proposal per file
  (`MoveProposal { name, target_subdir }`), deterministic, stable order.
- `OrganizeRule`: `ByKind` / `ByExtension` / `ByModifiedYear` / `BySizeBucket`.
- `find_clutter(entries) -> Vec<ClutterFinding>` (`ClutterReason` + `label()`).
- `OrganizeEntry::new(name, is_dir, ext, size, modified_secs)`.

Deliver it as a safe **propose → review → checkpoint → apply (undoable)** feature. Nothing moves until the user
approves in a preview; the apply is wrapped in a checkpoint so it's one-click undoable.

## Design
### Backend (thin commands into `cpe-server`, async + spawn_blocking)
- `organize_plan(dir, rule)` → list `dir` (reuse the existing directory-listing path), map each entry to an
  `OrganizeEntry`, return `plan_organize(...)`. Pure proposal — moves nothing.
- `organize_clutter(dir)` → same mapping → `find_clutter(...)` (optional secondary surface; include if cheap).
- `organize_apply(dir, rule)` (or take the already-computed proposals) → **first `checkpoint_create` the dir**
  (so the whole reorg is one undo), then for each proposal create `dir/<target_subdir>/` and move the file
  there, reusing the existing `move_exact`/`move_entries` primitives (grep them in `src-tauri/src/lib.rs`).
  Return an `OpResult`-style summary (moved / skipped-with-reason, skip-on-error per the fs convention).
- Regenerate `src/lib/bindings.gen.ts` (new specta commands → CI drift guard, see
  [[regen-specta-bindings-on-struct-change]]).

### Frontend (preview/approve UI)
- A menu/command entry ("Organize this folder…") opening a dialog: a **rule picker** (By kind / By extension /
  By year modified / By size), a **live preview** of the proposals grouped by target subdir (streamed or
  one-shot; debounce rule changes), a count, and **Apply** / **Cancel**. Apply calls `organize_apply`, shows a
  result toast, and surfaces the checkpoint so **Undo** is one click (reuse the checkpoint/rollback UI hooks).
- Dialog conventions: visible border, theme vars only, busy-cursor `invoke` wrapper, reflowing pills. Empty
  folder / zero-proposals / error states explicit.
- i18n: add all new keys to every locale (the CPE-481 100%-coverage gate).
- Docs: add an "Organize a folder" subsection to the relevant `src/docs/*.md` (no new `Section`).

## Acceptance Criteria
- [ ] `organize_plan`/`organize_apply` (+ `organize_clutter` if included) are registered commands (both
      `generate_handler!` + `collect_commands!`); `organize_plan` only proposes (moves nothing);
      `organize_apply` checkpoints first, then moves, returning a moved/skipped summary.
- [ ] The dialog previews proposals per rule, and Apply performs the moves and leaves a one-click Undo
      (checkpoint). Nothing moves without explicit Apply.
- [ ] Headless tests: command layer maps a temp tree → proposals for each rule; apply moves files into the
      right subdirs + a checkpoint exists for undo; a component/jsdom test covers the dialog logic (rule
      switch → preview, apply, empty/error states), backend mocked.
- [ ] `bindings.gen.ts` regenerated + committed; `npm run check` green; `crates/server` tests + clippy (both
      modes) green; `src-tauri` `cargo check` green.
- [ ] GUI-verified on the real build (build → deploy → run): pick a rule, preview looks right, Apply reorganizes
      the folder, Undo restores it. **Deferred to the Foreman + user pass.**

## Notes
- Shift-2 research pick (best value/effort "instant-index twin"): built+tested engine, unwired, no user
  resource needed for the rules mode. The **AI-assisted** organize mode (model-gated) stays out of scope.
- Because this MOVES user files, the checkpoint-protected apply + explicit preview/approve are mandatory — no
  silent or unconfirmed moves. Working this child re-activates epic CPE-979's rules slice.
