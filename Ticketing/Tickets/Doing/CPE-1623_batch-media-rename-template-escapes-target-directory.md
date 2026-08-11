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

---

## Work Log — PR #828, attempt 3: link-as-final-component defeats containment

A fresh audit re-tried every prior finding on this branch (attempt-2 IPC bypass, `shot..final`, `C:foo`,
extensionless `..`, `Convert.to_ext`) and could not break any of them — all confirmed still holding,
including that the lexical fallback fails **closed** when `canonicalize` fails on a >260-char path, and
that a non-existent input directory still fails closed. **One new finding, in the same function:**
`output_escapes_input_dir` (`batch_media.rs`) short-circuited at `if out_dir == dir { return false; }` — a
purely **textual** comparison of the two paths' directory portions. It never asked what `output`'s final
path component actually IS on disk. A link whose *name* sits inside the input's own directory can alias
data physically outside it, and the directory-text fast path waved that straight through with zero
resolution — even though `execute_plan_walk` calls this exact function as its "unconditional, not
waivable" gate.

**Both variants demonstrated on the real Windows filesystem, bytes re-read off disk:**

- **1a — hard link (no privilege needed, any Windows account, same volume).**
  `fs::hard_link(outside\important.jpg, selected\link.jpg)`, then a `PlannedItem{input:
  selected\photo.jpg, output: selected\link.jpg}` with `confirmed_overwrite: true` → `Ok(written:1)`, and
  `outside\important.jpg`'s bytes changed. Falsified the attempt-2 doc's claim that `confirmed_overwrite`
  can never license an out-of-folder write.
- **1b — dangling symlink (needs zero batch-job flags).** `create_symlink(target=outside\newly-
  planted.jpg /* doesn't exist yet */, link=selected\link.jpg)`. `Path::is_file()` on a dangling symlink
  is `false`, so `is_foreign_overwrite` sees "nothing there" and `confirmed_overwrite` stays at its
  default `false` → `Ok(written:1)` → a file created outside the selected folder with **no consent flag
  of any kind**. Also reachable from an ordinary UI-driven batch (not just the IPC bypass): a rename
  template producing the stem `link` reaches the identical `Path::is_file()` blind spot inside `plan()`'s
  own collision check.

**Fix, in `output_escapes_input_dir`:** the `out_dir == dir` fast path now calls a new helper,
`link_alias_escapes(output, input_dir, parent_cache) -> Option<bool>`, before trusting the text match:

- **Symlinks/junctions:** read the link's own stored target via `std::fs::read_link` (not
  `canonicalize`, which requires the WHOLE chain to exist and therefore fails outright on a dangling
  symlink — exactly the shape that needs zero flags to exploit). A relative target resolves against the
  link's own parent directory; the resolved location's directory is compared via the existing
  `path_key`/`ParentCache` machinery, so this stays O(n)-amortized — no new uncached `canonicalize` per
  item. An unreadable link (permission/race) fails closed (`Some(true)`), not open.
- **Hard links — deliberately NOT fully resolved.** A hard link has no "target" to read: every name for
  the same data is equally real, and there's no cheap way to enumerate a file's OTHER names short of
  walking every directory on the volume comparing (volume-serial, file-index) identity — disproportionate
  per planned item, and not attempted. Instead: if a real file sits at `output` and its **link count** is
  more than 1, some other name for the same data exists somewhere this fn cannot see — might be inside
  the selected folder, might not, no way to tell without the walk this deliberately skips. **Fails
  closed: refuses rather than guessing "probably fine."** This is the one gap left open **by design, not
  oversight** — a batch that writes through an existing multiply-linked file inside the selected folder
  is refused even when every other link is also harmlessly inside it, because there's no cheap way to
  know that. Link count is read via `std::os::windows/unix::fs::MetadataExt` on Unix (already-fetched
  metadata, zero extra syscall) — but Windows's `number_of_links()`/`file_index()`/
  `volume_serial_number()` are still gated behind the unstable `windows_by_handle` feature
  (rust-lang/rust#63010) on stable Rust, so Windows falls back to the raw Win32 call
  (`CreateFileW`+`GetFileInformationByHandle`) via the `windows` crate already vendored for
  `high_contrast.rs` — **no new dependency**, just two more feature flags (`Win32_Storage_FileSystem`,
  `Win32_Security`) on the existing pinned `windows = "0.56"` in `crates/server/Cargo.toml`. Confirmed no
  `Cargo.lock` change in either `crates/server` or `src-tauri` (same resolved crate version, just more of
  its already-vendored code compiled in).
- Returns `None` (falls through to the prior `false`, unchanged behaviour) when `output` doesn't exist
  yet, or exists as an ordinary single-linked file — the overwhelming common case, costing exactly one
  extra `symlink_metadata` stat, never a canonicalize.

**Negative control (actually run, not asserted):** temporarily reverted just the one fixed line
(`return link_alias_escapes(...).unwrap_or(false)` back to `return false`) and ran the new tests against
that state. All 5 new regression tests failed exactly as predicted — `link_as_final_component_hard_link_
alias_is_refused_even_with_confirmed_overwrite` observed `Ok(BatchReport { written: 1, skipped: [] })`
(the hard-link write actually went through), `link_as_final_component_dangling_symlink_is_refused_with_
no_confirmation_needed` likewise, plus the three `batch_media.rs` unit-level assertions on
`output_escapes_input_dir` itself. Restored the real fix, reran — all green (see full-suite numbers
below).

**New tests:**
- `batch_media.rs` (unit-level, direct calls to `output_escapes_input_dir`): `cpe_1623_hard_link_alias_
  within_the_same_directory_text_escapes`, `cpe_1623_dangling_symlink_alias_within_the_same_directory_
  text_escapes`, `cpe_1623_live_symlink_alias_within_the_same_directory_text_escapes` (extra coverage
  beyond the two demonstrated PoCs — the fix is general to any symlink final component), `cpe_1623_
  symlink_pointing_back_inside_the_same_directory_does_not_escape` (no false positive: a symlink whose
  target legitimately stays in-folder), `cpe_1623_ordinary_pre_existing_output_file_is_unaffected` (no
  false positive: a plain existing file, no link involved, handled exactly as before).
- `batch_execute.rs` (end-to-end via `execute_plan`, byte-level proof off disk): `link_as_final_component_
  hard_link_alias_is_refused_even_with_confirmed_overwrite`, `link_as_final_component_dangling_symlink_
  is_refused_with_no_confirmation_needed`. Symlink tests skip cleanly (not fail) if the environment can't
  create a symlink (Windows Developer Mode) — this machine has it enabled; CI may not.
- `cpe_1623_execute_plan_walk_containment_recheck_stays_bounded` (`batch_execute.rs`) — the "ALSO" item:
  there was a perf-regression guard for `plan()`'s copy of the containment check
  (`cpe_1623_containment_check_for_a_directory_changing_rename_stays_bounded`) but none for `execute_plan_
  walk`'s own copy of the identical check. Mirrors the same "./{stem}-renamed" trick (forces the
  `path_key` resolution branch on every item, not the near-zero-cost fast path) at the execute layer,
  same O(n) bound. Required promoting `reset_canonicalize_call_count`/`canonicalize_call_count` from
  private to `pub(crate)` (still `#[cfg(test)]`-only) so a test in `batch_execute.rs` can share the same
  counter.

**Explicitly out of scope, untouched:** the ADS/colon case (confirmed still template-only, stays with
CPE-1624 unchanged); CPE-1624's TOCTOU per-write re-check; `is_foreign_overwrite`'s pre-existing O(n)
scan; the cosmetic `lexical_normalize` drive-letter double-prefix quirk (confirmed to produce distinct
keys, not a fail-open bug).

**Verification (all run synchronously, this environment):**
- `cargo build` (crates/server) — clean. `cargo build` (src-tauri) — clean.
- `cargo test --lib` (crates/server) — **1896 passed**, 0 failed, 2 ignored (baseline 1888 + 8 new tests:
  5 in `batch_media.rs`, 3 in `batch_execute.rs`).
- `cargo clippy --all-targets -- -D warnings` (crates/server; default, `--features index`, `--features
  specta`) — all clean, 0 warnings.
- `cargo clippy --all-targets -- -D warnings` (src-tauri) — clean, 0 warnings.
- `npm run check` — 0 errors, 0 warnings.
- `npx vitest run` — **273 files, 3337 tests passed**, 0 failed (no frontend files touched this pass).
- `cpe_1623_plan_timing_for_2000_files` (release, `--ignored --nocapture`): **219.01ms**, vs. the prior
  ~164ms baseline — a real but small (~27µs/file) increase from the new `symlink_metadata` stat this fix
  adds to the `out_dir == dir` fast path; stays clearly linear, not a regression toward quadratic.
- Canonicalize-count guards green: both `cpe_1613_plan_collision_check_makes_a_bounded_number_of_
  canonicalize_calls_not_quadratic` and `cpe_1623_containment_check_for_a_directory_changing_rename_
  stays_bounded`, plus the new `cpe_1623_execute_plan_walk_containment_recheck_stays_bounded`.
- Files changed this pass: `crates/server/src/batch_media.rs`, `crates/server/src/batch_execute.rs`,
  `crates/server/Cargo.toml` (two new `windows` crate features on the already-pinned 0.56 dependency, no
  new dependency, no `Cargo.lock` change in either `crates/server` or `src-tauri`).
