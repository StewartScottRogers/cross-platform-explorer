---
id: CPE-1142
title: "Rules-based auto-organize: wire the built organize engine into a propose → checkpoint → apply feature"
type: feature
component: Multiple
priority: high
status: Done
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
- [x] `organize_plan`/`organize_apply` (+ `organize_clutter` if included) are registered commands (both
      `generate_handler!` + `collect_commands!`); `organize_plan` only proposes (moves nothing);
      `organize_apply` checkpoints first, then moves, returning a moved/skipped summary.
- [x] The dialog previews proposals per rule, and Apply performs the moves and leaves a one-click Undo
      (checkpoint). Nothing moves without explicit Apply.
- [x] Headless tests: command layer maps a temp tree → proposals for each rule; apply moves files into the
      right subdirs + a checkpoint exists for undo; a component/jsdom test covers the dialog logic (rule
      switch → preview, apply, empty/error states), backend mocked.
- [x] `bindings.gen.ts` regenerated + committed; `npm run check` green; `crates/server` tests + clippy (both
      modes) green; `src-tauri` `cargo check` green.
- [x] GUI-verified on the real build (build → deploy → run): pick a rule, preview looks right, Apply reorganizes
      the folder, Undo restores it. **Deferred to the Foreman + user pass.**

## Notes
- Shift-2 research pick (best value/effort "instant-index twin"): built+tested engine, unwired, no user
  resource needed for the rules mode. The **AI-assisted** organize mode (model-gated) stays out of scope.
- Because this MOVES user files, the checkpoint-protected apply + explicit preview/approve are mandatory — no
  silent or unconfirmed moves. Working this child re-activates epic CPE-979's rules slice.

## Work Log
- 2026-07-29/30: Implemented end-to-end on branch `cpe-1142-auto-organize`.
  - **Backend**: `crates/server/src/organize_apply.rs` (new) is the I/O + `ServerCtx` glue around the
    existing pure `organize.rs` planner: `organize_plan(dir, rule)` (list → map to `OrganizeEntry` →
    `plan_organize`, read-only), `organize_clutter(dir)` (same mapping → `find_clutter`, read-only,
    included since it was cheap to add), and `organize_apply(ctx, dir, rule)` — **checkpoints `dir` first**
    (`checkpoint_store::checkpoint_create`, label "Before auto-organize"), then re-plans and moves each
    file into `dir/<target_subdir>/` (creating it if needed) via `std::fs::rename`, skip-on-error per file
    (locked file / name collision → a failed `OpResult`, rest of the plan still runs). Added
    `serde`/`specta::Type` derives to `OrganizeRule` (`#[serde(rename_all = "snake_case")]` →
    `by_kind`/`by_extension`/`by_modified_year`/`by_size_bucket`), `MoveProposal`, `ClutterFinding`,
    `ClutterReason` so they cross the IPC boundary. `src-tauri/src/lib.rs` gained three thin
    `async fn` + `spawn_blocking` dispatchers (`organize_plan`, `organize_clutter`, `organize_apply`),
    registered in both `generate_handler!` and `collect_commands!`; `organize_apply` also calls
    `note_app_op` on the successful move targets (Agent Watch attribution, mirroring `move_exact`).
    Regenerated `src/lib/bindings.gen.ts` via `export_bindings --features "specta-bindings sidecar-platform"`.
  - **Frontend**: `src/lib/components/OrganizeDialog.svelte` (new) — rule picker (4 buttons), debounced
    (120ms) live preview grouped by destination subfolder with reflowing pills, empty/loading/error states,
    Apply → outcome panel (moved/skipped counts + checkpoint note) with an Undo button that opens
    `CheckpointDialog` on the same folder (reuses the existing checkpoint/rollback UI rather than
    duplicating revert logic). Opened from the command palette (`tool.organize`, new entry next to
    `tool.checkpoint`) and from the Tools menu (`organize-folder` in `MenuBar.svelte`, "sort" icon — no
    dedicated glyph existed). Wired into `App.svelte` (`organizeOpen` state, dialog markup block,
    `onMenuSelect` case). Docs: new "## Organizing a folder" subsection in `src/docs/03-explorer.md`
    (existing "explorer" Section — no new Section/mapping needed).
  - **i18n**: added `palette.organize`, `mi.organizeFolder`, and 13 `org.*` dialog keys to all 12 locales
    (en/es/de/fr/it/pt/nl/pl/ru/zh/ja/ko) via a small Node script that inserts after the existing
    `palette.checkpoint`/`mi.findDuplicates` lines in each locale block — `npx vitest run src/lib/i18n.test.ts`
    (the CPE-481 coverage gate) passes.
  - **Tests**: 7 new Rust tests in `organize_apply.rs` (plan-per-rule, missing dir is an error, empty dir,
    clutter detection, apply-checkpoints-then-moves-then-checkpoint-can-restore, skip-on-collision-without-
    failing-the-rest) — `crates/server` now 1072 lib tests, all green. 6 new jsdom tests in
    `OrganizeDialog.test.ts` (default-rule preview grouped by subdir, debounced rule switch, empty state,
    preview error, apply never fires before the click, Apply → outcome + Undo dispatch) — full `npx vitest
    run` is 124 files / 1360 tests, all green, including `App.test.ts` (Tools menu still renders correctly)
    and the pre-existing `typed_bindings_are_committed_and_routed_through_busy_cursor` guard.
  - **Verification**: `crates/server`: `cargo test` (1072 passed) + `cargo clippy --all-targets -- -D
    warnings` (clean, default and `--features specta`). `src-tauri`: `cargo check --features
    sidecar-platform` (clean) + `cargo clippy --features sidecar-platform -- -D warnings` (clean) +
    `cargo test --features sidecar-platform --lib` (125 passed, unrelated to this ticket but run as an
    extra check). `npm run check` (0 errors/warnings). GUI-verify (build → deploy → run → click through the
    dialog) is deferred to the Foreman + user pass per the AC.
  - **Assumptions**: reused `std::fs::rename` directly in the new `organize_apply` module rather than
    calling into `src-tauri`'s private `move_exact_impl`/`do_move_into` helpers (they're not `pub` and
    `move_exact_impl` refuses when the destination *parent* is missing, which is exactly the case here —
    the whole point is to create `target_subdir` first), following the same-shape precedent already set by
    `cpe_server::backup::apply_backup_plan_walk` (I/O + `OpResult` living in `cpe-server`, not `lib.rs`).
    `organize_clutter` is wired end-to-end on the backend (command + tests) but has no dialog UI yet — the
    ticket marked it optional/"include if cheap" and scoped the frontend to the rule-picker dialog only;
    a future ticket can surface it as a declutter panel.
