---
id: CPE-1623
title: "Batch Media's rename template accepts path separators, so `..\\..\\elsewhere\\name` silently overwrites an arbitrary file — from the normal UI, in \"non-destructive\" mode, with no confirmation"
type: Bug
status: Backlog
priority: High
component: Backend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the independent Security Auditor while threat-modelling CPE-1613 (PR #818). **Not introduced by
that PR** — the flaw predates it (CPE-940 / CPE-723) — but CPE-1613's work disproves the module's own
claim that `plan()` "keeps output non-destructive" and that the engine "refuses" destructive writes. It
is filed High because it was **demonstrated end-to-end on a real Windows box** and is reachable by an
ordinary user typing into an ordinary text field.

## The bug
`plan()`'s rename path substitutes the user's template into the output stem with **zero sanitisation**:

- `crates/server/src/batch_media.rs:305-309` — `template.replace("{stem}", &stem).replace("{n}", ...).replace("{ext}", ...)`.
  Nothing rejects `/`, `\`, or `..`.
- `validate()` (`batch_media.rs:113-138`) only checks the template is **non-empty**.
- `join(&dir, &out_stem, &ext)` (`batch_media.rs:151-157`) is plain string concatenation — no lexical containment.
- `plan()`'s non-destructive collision guard (`batch_media.rs:332-343`) compares the computed output only
  against `used` — *this batch's own inputs and outputs* — via `same_file`. It never checks whether the
  computed output already exists on disk as an unrelated file outside the batch.
- `execute_one` (`batch_execute.rs:96-105`) then performs an unconditional `fs::write(&item.output, ...)`.

## Demonstrated reproduction (actually run, not hypothesised)
Input `traversal_workdir\innocuous.jpg` (a real, valid JPEG). Job: `MediaOp::Rename { template:
"..\\..\\cpe1613_traversal_victim\\important" }`, `non_destructive: true` — the **default, supposedly safe**
mode ("write to new files" checked). An unrelated pre-existing file sat at
`...\cpe1613_traversal_victim\important.jpg` containing `"VICTIM ORIGINAL CONTENT - must not be touched"`.

Result: `plan()` computed an output that `std::fs::canonicalize` resolved to a path **outside the input's
own directory**. `any_in_place(&items)` returned `false` — correctly, because they genuinely *are*
different files, so `same_file` is not even wrong here. `execute_plan(&items, &job)` returned
`Ok(BatchReport { written: 1, skipped: [] })` with **no error and no refusal**, and reading the victim file
back showed its contents replaced with re-encoded JPEG bytes. The original was gone.

## Why it matters
- **Reachable from the shipped UI, not just devtools.** `src/lib/components/BatchMediaDialog.svelte:451` is
  a bare `<input bind:value={renameTemplate}>` with no character restriction; the only frontend check is
  "non-empty" (`BatchMediaDialog.svelte:116-117`).
- **The confirmation never fires.** Because input and output are genuinely different files,
  `overwritesInPlace` / `any_in_place` never trigger, so the "Overwrite N files" dialog never appears. The
  write proceeds as an ordinary, un-flagged "Apply".
- The entire `confirmed_overwrite` guard machinery (CPE-1590/1599/1613) is scoped to *"does this collide
  with the batch's own input/output"* — **never** to *"does this stay inside the folder the user picked"*.

## Fix
1. **Constrain every computed output to the selected directory.** Lexically normalise the joined path and
   reject (or refuse the whole batch) if it does not remain under the batch's target dir. This is the real
   fix — do it in `plan()` so both the collect and streaming paths inherit it.
2. **Reject path separators and `..` in the rename template** at `validate()`, with a clear user-facing
   error, and mirror the restriction in `BatchMediaDialog.svelte` so the user is told before they click.
3. **Extend the destructive check beyond the batch's own set:** an output that resolves onto an existing
   file *not* part of this batch must be treated as an overwrite — requiring confirmation, and refused by
   the engine when unconfirmed, exactly as CPE-1599 established.
4. Correct the module docs' claim about `plan()` being non-destructive once the above holds.

## Acceptance criteria
- The demonstrated reproduction above **fails to escape**: the batch is refused (or the output is contained),
  the victim file is byte-for-byte untouched, and the user is told why.
- A test reproduces the traversal against the **old** code (negative control) and shows it blocked on the new.
- Ordinary rename templates (no separators) are unaffected — no new false alarms.
- An output landing on an existing unrelated file requires confirmation and is engine-refused without it.
- Frontend and engine agree on what a valid template is; the engine remains the enforcement point.

**Conflict surface:** `crates/server/src/batch_media.rs`, `crates/server/src/batch_execute.rs`,
`src/lib/components/BatchMediaDialog.svelte`, `src/lib/batchMedia.ts`, plus their tests.

## Notes
Ranked by the auditor as by far the most severe of three findings; the other two are filed as CPE-1624.

## Work Log

**Fix, in `crates/server/src/batch_media.rs`:**

- **`validate()`** rejects a `Rename` template containing `/`, `\`, or a literal `..` (new
  `template_escapes_directory` helper) with a clear error, before `plan()` is ever called from the one
  production path (`batch_media_plan`).
- **`plan()` now returns `Result<Vec<PlannedItem>, String>`** (was infallible) and refuses the WHOLE batch
  — mirroring the CPE-1590/1599 "refuse, don't guess" treatment of a destructive write, not the ordinary
  per-file skip-on-error path — when a computed output's directory (after the Rename substitution) doesn't
  match the input's own directory. Reuses CPE-1613's `path_key` for the comparison; a plain string check
  (`out_dir != dir`) short-circuits the common case (every non-Rename op, and any separator-free Rename)
  so this costs nothing extra for an ordinary batch. This is the actual containment fix — `validate()` is
  the friendlier early warning, `plan()` is the enforcement point a caller can't bypass by skipping
  `validate()`.
- **Real-filesystem non-destructive guarantee:** the non-destructive collision-avoidance loop now also
  disambiguates (`-2`, `-3`, …) around a REAL pre-existing file this batch never selected, not just a
  collision with the batch's own working set — a single `Path::is_file()` stat per item, not a
  `canonicalize`, so the common "name is free" case adds one cheap syscall, no O(n²) regression.
- Module doc corrected: `plan()` is no longer described as filesystem-free or unconditionally
  non-destructive; the containment guarantee and its scope are spelled out.

**Fix, in `crates/server/src/batch_execute.rs`:** `any_in_place`/`execute_plan_walk`'s refusal broadened
from "output == own input" to `is_foreign_overwrite` — also true when the output resolves onto a REAL
file that isn't any input in this batch. Refused without `confirmed_overwrite`, allowed once it's set,
exactly like the existing in-place case (CPE-1599). `Path::is_file()` gates the expensive same-batch scan
so the common non-colliding case stays cheap.

**Frontend, in `batchMedia.ts` / `BatchMediaDialog.svelte`:** new `templateEscapesDirectory()` mirrors the
backend's separator/`..` rule; the Rename field's "+ Add" disables and an inline hint explains why *before*
the user can even submit it. New `bm.renameEscapes` i18n key added to all 12 complete locale catalogs
(en/es/de/fr/it/pt/nl/pl/ru/zh/ja/ko) in `i18n.ts` — the 100%-coverage guard test enforces this.

**`batch_media_plan` (src-tauri/src/lib.rs)** wrapped in `spawn_blocking`: `plan()` now does genuine
blocking I/O (canonicalize + stat), which it didn't when this dispatcher was first written async-without-
spawn_blocking — per CPE-760/761's convention. `bindings.gen.ts` regenerated (doc-comment only change on
the exported command; no type/struct changed).

**Reproduced the ticket's exact scenario, with real files, bytes read back (not trusted from a return
value):** `traversal_workdir/innocuous.jpg` (real JPEG) two levels under a scratch root, sibling
`cpe1613_traversal_victim/important.jpg` seeded with `"VICTIM ORIGINAL CONTENT - must not be touched"`
(45 bytes). `plan()` with `Rename { template: "..\\..\\cpe1613_traversal_victim\\important" }`,
`non_destructive: true` now returns `Err(...)` containing "folder" — nothing is ever read or written.
`fs::read` of the victim afterward: still the original 45 bytes, byte-for-byte. Test:
`cpe_1623_directory_traversal_rename_is_refused_with_real_files_on_disk` in `batch_media.rs`.

**Negative control (temporary, not shipped):** stashed the fix, added a throwaway test to the pre-fix
`batch_execute.rs` reproducing the identical scenario against the OLD infallible `plan()`. Observed:
`plan()` computed `...\traversal_workdir\..\..\cpe1613_traversal_victim\important.jpg`, resolving (via
`canonicalize` of its parent) exactly onto `cpe1613_traversal_victim`; `any_in_place` was `false`
(different files — correctly, per the ticket); `execute_plan` returned `Ok(BatchReport { written: 1, .. })`
with no error; the victim's bytes changed from 45 bytes to 163 bytes of valid, decodable JPEG data —
the original content was gone. Confirms the vulnerability is real and that the new test would have failed
against the old code. Discarded the throwaway test (`git checkout --`) and popped the stash to restore the
real fix before continuing.

**No regressions — explicit coverage:**
- `cpe_1623_ordinary_rename_templates_without_separators_are_unaffected` /
  `cpe_1623_validate_rejects_rename_templates_with_separators_or_traversal` (ordinary templates like
  `{stem}`, `{stem}-{n}`, `photo-{n}` still plan/validate exactly as before).
- `cpe_1623_non_destructive_mode_steps_around_a_real_pre_existing_unrelated_file` (non-destructive mode
  picks `vacation-2.jpg` instead of silently overwriting a real pre-existing `vacation.jpg`).
- `cpe_1623_output_landing_on_a_foreign_existing_file_is_refused_then_allowed_once_confirmed` /
  `cpe_1623_overwrite_mode_with_a_genuinely_new_output_name_needs_no_confirmation` (overwrite-mode
  collision with a foreign file refused → allowed once confirmed; a genuinely new name needs no
  confirmation) in `batch_execute.rs`.
- The existing CPE-1613 perf regression guard (`cpe_1613_plan_collision_check_makes_a_bounded_number_of_
  canonicalize_calls_not_quadratic`, n=300) still passes with the new containment/foreign-file checks in
  place.

**Measured `plan()` timing, 2000 files in one directory, release build (`cpe_1623_plan_timing_for_2000_
files`, `#[ignore]`d — run manually with `cargo test --release -- --ignored --nocapture`):**
**186.16ms** — in the same range as CPE-1613's post-fix ~196ms baseline the ticket cites; the new
containment fast-path and `is_file()` stat add no measurable regression.

**Out of scope, left for CPE-1624 (per the ticket's own instruction — "work them in sequence"):** the
TOCTOU per-write re-check, and alternate-data-stream path recognition. Neither touched here.

**Verification (all run synchronously, this environment):**
- `cargo build` (crates/server) — clean.
- `cargo build` (src-tauri) — clean.
- `cargo test` (crates/server, full suite) — **1878 passed**, 0 failed, 2 ignored (the new timing test +
  1 pre-existing unrelated).
- `cargo clippy --all-targets -- -D warnings` (crates/server, default features) — clean, 0 warnings.
- `cargo clippy --all-targets --features specta -- -D warnings` (crates/server) — clean, 0 warnings.
- `cargo clippy --all-targets -- -D warnings` (src-tauri) — clean, 0 warnings.
- `npm run check` — 0 errors, 0 warnings.
- `npx vitest run` — **273 files, 3337 tests passed**, 0 failed.
- `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` — regenerated
  `bindings.gen.ts`; diff is the doc-comment change only (no type/struct changed).
- Files changed: `crates/server/src/batch_media.rs`, `crates/server/src/batch_execute.rs`,
  `src-tauri/src/lib.rs`, `src/lib/batchMedia.ts` (+ its test), `src/lib/components/BatchMediaDialog.svelte`,
  `src/lib/i18n.ts`, `src/lib/bindings.gen.ts`, `src/docs/explorer-batch-media.md`. No `Cargo.toml`/
  `Cargo.lock` change, no new dependency.
