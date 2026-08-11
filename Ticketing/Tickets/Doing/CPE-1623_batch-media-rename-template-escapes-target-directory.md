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

---

## Work Log — PR #828, blocking-finding pass (three rounds)

PR #828 (branch `cpe-1623-contain-batch-output`) was already open with the fix above when an independent
Security Auditor demonstrated the engine — not just the UI — was still bypassable, followed in the same
pass by a UAT false-positive report and, after fixing that, a Reviewer's CHANGES REQUESTED with two more
engine-level bypasses. All fixed in this one pass, on the same branch/PR.

### Finding 1 (Security Auditor) — the IPC surface never re-derives containment

**The gap:** every containment check lived inside `batch_media::validate()`/`plan()`. But
`batch_media_execute_stream` (`src-tauri/src/lib.rs`) takes `items: Vec<PlannedItem>` straight from the IPC
caller, and `PlannedItem` is a plain public struct with zero invariants (`Serialize`/`Deserialize`, nothing
enforcing "came from `plan()`"). `execute_plan_walk`'s only gate, `is_foreign_overwrite`
(`batch_execute.rs`), asked "does something already exist at this output?" — never "does this stay inside
the input's own folder?" — so a hand-built `PlannedItem` with `output` pointing anywhere the process can
write sailed through as a **new** file, `confirmed_overwrite` or not.

**Fix:** pulled the containment check out of `plan()` into a new shared fn,
`batch_media::output_escapes_input_dir(input, output, &mut ParentCache)` (same `path_key`-based directory
identity comparison `plan()` always used — `ParentCache` is a new `pub(crate)` type alias for the memoized
cache so it can be threaded across a whole batch from either caller). `plan()` now calls it instead of
duplicating the logic. `execute_plan_walk` now calls it too, per item, **before any byte is read or
written**, unconditionally (not gated behind `confirmed_overwrite` — that flag only ever authorises
overwriting the user's own input in place, never escaping the folder). Refuses the whole batch, nothing
written, on any escaping item.

**Negative control (actually run, not asserted):** wrote the regression tests first, ran them against the
branch's HEAD (which had the CPE-1623 planner fix but NOT this IPC-bypass fix) — both hand-built-escaping
tests failed exactly as predicted: `execute_plan` returned `Ok(BatchReport { written: 1, skipped: [] })`
for a `PlannedItem` whose `output` pointed at `.../cpe1613_traversal_victim/important.jpg`, built without
ever calling `plan()`. Applied the fix, reran — both refuse with "outside its own input's folder", nothing
written, and (for the `confirmed_overwrite: true` case) a real pre-existing victim file's bytes
byte-for-byte unchanged. New tests:
`ipc_bypass_hand_built_escaping_planned_item_is_refused_and_writes_nothing`,
`ipc_bypass_hand_built_escaping_planned_item_is_refused_even_with_confirmed_overwrite`,
`ipc_bypass_containment_recheck_does_not_disturb_an_ordinary_contained_plan` (no false alarms) — all in
`batch_execute.rs`.

### Finding 2 (UAT) — `..` substring check over-corrected: `shot..final` wrongly rejected

**The gap:** `template_escapes_directory()` (`batch_media.rs`) and its frontend mirror
(`templateEscapesDirectory()` in `batchMedia.ts`) rejected `..` as a raw **substring**, so an ordinary
filename like `"shot..final"` or a version stamp `"v1..2"` — no separator anywhere, so nothing to walk
through — was wrongly refused. Directly violated the ticket's own acceptance criterion ("ordinary rename
templates, no separators, are unaffected").

**Fix:** `..` is now a traversal risk only as a **whole path segment**. Once a separator (`/` or `\`, and
now `:` — see Finding 3) has already failed the check and returned `true`, the template is guaranteed to
contain none of them, so "is `..` a whole segment" reduces to "is the (trimmed) template exactly `..`" —
every genuine traversal case the module's tests already pinned (`".."`, `"../evil"`, `"..\\evil"`,
`"a/../b"`) still contains a separator and is still rejected; `"shot..final"`/`"v1..2"`/`"a..b"`/`"..."`
now validate fine. Backend and frontend kept in lockstep (same rule, same character set).

**Negative control:** isolated the "shot..final" case in its own throwaway test against the pre-fix
substring check — confirmed it failed (`validate()` rejected it) before applying the narrowed rule, then
removed the throwaway test once the real fix (and the permanent test,
`cpe_1623_dotdot_only_rejected_as_a_whole_path_segment_not_any_occurrence`) confirmed green.

**Also resolved the auditor's inconclusive Unicode-slash question while here:** U+2215 (DIVISION SLASH),
U+FF0F (FULLWIDTH SOLIDUS), U+FF3C (FULLWIDTH REVERSE SOLIDUS) are **accepted**, definitively — they are
distinct Unicode scalars from ASCII `/`/`\`, the `char`-based `contains` check correctly doesn't match
them, `split`/`join` only ever split on the literal ASCII characters, and none of the three is one of
NTFS's 9 reserved characters (`< > : " / \ | ? *`) — so a template containing one produces an ordinary,
real, single-file output inside the input's own directory. Proven with real files on disk, not just string
assertions: `cpe_1623_unicode_lookalike_slash_characters_are_accepted_not_path_separators`.

### Finding 3 (Reviewer, CHANGES REQUESTED, attempt 2) — two structural gaps in the directory-identity check

The reviewer confirmed the core containment logic (canonicalized directory identity via `path_key`, not a
string prefix), the junction/UNC/`\\?\` handling, `spawn_blocking`, the zero-diff bindings regen, and the
12 real locale translations were all sound — but found two ways `split()` (a plain filename splitter, not
a full path parser) let a crafted output slip past the directory-identity comparison itself:

- **Finding A — `C:foo` on a bare-filename input.** `split` finds `dir == ""` for a bare relative filename
  (no directory component at all). A template like `"C:foo"` produced an `output` whose computed
  directory was *also* textually empty — two empty strings compared equal, no escape detected — even
  though `C:foo.jpg` is a Windows drive-relative reference resolving against drive `C:`'s own current
  directory at write time, not the folder the user picked. Live repro:
  `plan(Rename{template:"C:foo"}, ["innocuous.jpg"])` → `Ok([...output: "C:foo.jpg"...])`.
- **Finding B — a bare `..`/`.` final component.** An extensionless input plus a template that's literally
  `".."` produced an `output` whose FINAL path component was a bare `..` — `split` hands that back as an
  ordinary-looking "stem", so the directory-identity check never even asked whether it denotes a real
  file. Live repro: `plan(Rename{template:".."}, ["/pics/a/traversal_workdir/innocuous"])` →
  `Ok([...output: "/pics/a/traversal_workdir/.."...])`. (Harmless with an EXTENSIONED input — `join`
  appends the extension, turning the `".."` stem into the literal filename `"...ext"` — confirmed by test.)
- **Finding C (message accuracy)** — a `Convert.to_ext` escape fed straight to `plan()` (bypassing
  `validate()`) was still refused (the containment check is op-agnostic), but the message read "rename
  template" regardless of which op caused it.

**Fix:** `output_escapes_input_dir` now checks, in order: (1) the output's raw final path component (via
`output.rsplit(['/','\\'])`) — if it's exactly `"."` or `".."`, refuse outright, independent of the
directory comparison (closes Finding B); (2) if the input's `dir` is empty AND the output's stem contains
`:`, refuse (closes Finding A's structural gap); (3) the existing directory-identity comparison. Also
rejected `:` outright in `template_escapes_directory()` (closes Finding A at the friendlier field-level
layer for both Rename and Convert — also incidentally narrows the NTFS alternate-data-stream surface,
though that finding stays out of scope, filed as CPE-1624). `plan()`'s backstop error message reworded to
name **both** possible causing ops ("a Convert extension or Rename template can only change...") instead of
hard-coding "rename template" (closes Finding C).

**Negative control:** temporarily stripped just the two new guard blocks from `output_escapes_input_dir`
(reproducing the exact pre-fix directory-comparison logic) and reverted `template_escapes_directory` to
the original unconditional substring check; ran the new tests against that state. Observed exact repro of
both reviewer PoCs: `cpe_1623_reviewer_finding_a_bare_filename_input_drive_relative_template_is_refused`
failed with `output: "C:foo.jpg"` accepted; `cpe_1623_reviewer_finding_b_dotdot_final_component_is_refused`
failed with `output: "/pics/a/traversal_workdir/.."` accepted; `cpe_1623_validate_rejects_rename_templates_
with_separators_or_traversal` and the new Convert-extension test both failed on `"C:foo"`/`"secrets:hidden"`
being accepted. Restored the real fix, reran — all green.

**Perf-guard blind spot (reviewer caveat):** the existing quadratic-regression guard
(`cpe_1613_plan_collision_check_makes_a_bounded_number_of_canonicalize_calls_not_quadratic`) uses
`MediaOp::Compress`, which never enters the containment branch at all (`out_dir == dir` textually for
every item). Added a new test, `cpe_1623_containment_check_for_a_directory_changing_rename_stays_bounded`,
using a `Rename` template of `"./{stem}"` (changes `out_dir` textually — adds a `"./"` segment — but
resolves to the identical real directory), so `path_key`'s full resolution genuinely runs on every item;
confirmed still O(n) (n=300, bound n×10 canonicalize calls) rather than extending the original guard
in-place (kept them separate to avoid destabilising an already-established, passing test).

**Verification (all run synchronously, this environment, after all three rounds):**
- `cargo build` (crates/server) — clean. `cargo build` (src-tauri) — clean.
- `cargo test --lib` (crates/server) — **1888 passed**, 0 failed, 2 ignored (baseline 1878 + 10 new tests:
  7 in `batch_media.rs`, 3 in `batch_execute.rs`).
- `cargo clippy --all-targets -- -D warnings` (crates/server, default + `--features specta`) — both clean.
- `npm run check` — 0 errors, 0 warnings.
- `npx vitest run` — **273 files, 3337 tests passed**, 0 failed (includes `batchMedia.test.ts`'s expanded
  accept/reject sets and the i18n 100%-coverage gate for the new `bm.convertEscapes` key).
- `cargo run --bin export_bindings --features specta-bindings,sidecar-platform` — regenerated
  `bindings.gen.ts`; content is byte-identical (line-ending-normalized diff empty) — no `specta::Type`
  struct changed, so no commit needed for it.
- `cpe_1623_plan_timing_for_2000_files` (release, `--ignored --nocapture`): **164.19ms** — in line with
  (in fact faster than) the prior measured baseline of ~186ms; no regression from the new checks.
- Canonicalize-count guard (`cpe_1613_plan_collision_check_makes_a_bounded_number_of_canonicalize_calls_
  not_quadratic`) still green, plus the new containment-branch-specific guard above.
- Files changed this pass: `crates/server/src/batch_media.rs`, `crates/server/src/batch_execute.rs`,
  `src/lib/batchMedia.ts`, `src/lib/batchMedia.test.ts`, `src/lib/components/BatchMediaDialog.svelte`,
  `src/lib/i18n.ts`, `src/docs/explorer-batch-media.md`. No `Cargo.toml`/`Cargo.lock` change, no new
  dependency, no `bindings.gen.ts` commit needed (zero diff).
