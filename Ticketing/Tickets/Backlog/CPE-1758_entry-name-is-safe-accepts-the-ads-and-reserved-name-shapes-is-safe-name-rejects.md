---
id: CPE-1758
title: entry_name_is_safe accepts the ADS and reserved-name shapes is_safe_name rejects
type: task
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-15
closed:
---

## Why this exists

Split out of **CPE-1744** by the worker that closed its containment half, so the remainder is scheduled
rather than left as a sentence. CPE-1744 asked whether `crate::transfer::guarded_join` could be adopted
wholesale at `archive.rs`'s extraction sinks. **Half of it was**, and half deliberately was not:

- `guarded_join`'s **filesystem-resolving** half — does the joined path still land under the base once
  every intermediate component is followed — was the intermediate-directory escape, and CPE-1744 closed
  it at rows 15/16/18/19/20 via `fsutil::confined_to`.
- `guarded_join`'s **per-segment name** half is this ticket. It applies
  `crate::transfer::is_safe_name` to each segment (fails closed on a `:` anywhere and on a leading `..`,
  CPE-1461/1709) and on Windows sanitises through `local_safe_segment`. `archive::entry_name_is_safe`
  has **no equivalent to either**, and that is a question about what a name may *be*, not about where a
  path *lands* — a different guard with a different blast radius, shared with the `local_safe_segment`
  family rather than with `confined_to`.

## The measurement (from PR #906's UAT, unchanged)

```text
[M7] entry_name_is_safe("file:stream") = true    entry_name_is_safe("..evil") = true
     entry_name_is_safe("con") = true            entry_name_is_safe(" sp ") = true    ("x." = true)
[M8 fs::write to "adsbase:stream"] = Ok(())
     adsbase_len = Some(4) (unchanged)   a plain file named "adsbase:stream" exists = false
```

A ZIP entry named `file:stream` passes `entry_name_is_safe`, reaches rows 15–16's `File::create`, and on
NTFS the bytes land in an **alternate data stream of a neighbouring file** — the user is shown a
successful extraction and has no file. That is CPE-1709's bug at a sink CPE-1709 did not cover. The
Windows reserved-device names (`con`, `nul`, …) and the trailing-space/trailing-dot shapes are accepted
too.

## What to do

- [ ] Decide and write down first: adopt `local_safe_segment`/`is_safe_name` per segment at
      `entry_name_is_safe`, or a third implementation. Adopting is strongly preferred — three
      implementations of "is this leaf name safe" is how `deny_stat_of` ended up needing the same fix
      three times.
- [ ] Note the **rename vs refuse** decision explicitly: `local_safe_segment` *sanitises* (the transfer
      sink renames the file so the bytes still arrive), while `entry_name_is_safe` *refuses* (the entry is
      skipped). Extraction may want the sanitising behaviour — an entry silently dropped is the same
      "successful extraction, no file" outcome the ADS bug produces. Whichever is chosen, the in-app docs'
      zip-slip bullet describes the current one and must move with it.
- [ ] `entry_name_is_safe` is `pub(crate)` and `crate::extract_plan` reuses it (CPE-1055) — check that
      caller before changing the contract.
- [ ] **`archive::tests::entry_name_is_safe_accepts_shapes_transfers_is_safe_name_rejects` will go red.
      That is the intended signal.** Re-point it at the new behaviour in the same commit; never delete it.
      The `archive.rs` section-comment paragraph that records this delta, and the table above it, move in
      the same commit too.
- [ ] Every guard broken on its own, a **distinct** test red, real output pasted. Assert on the filesystem
      and the bytes **before** unwrapping the `Result` — this whole family fails by returning `Ok`.
- [ ] Pin a **distinctive** refusal, never `is_err()`: on Windows several of these shapes make
      `File::create` fail by itself, so an `is_err()` leg passes straight through a deleted guard.

## Notes

Filed by the CPE-1744 worker, 2026-08-15. Related: **CPE-1744** (the containment half, closed),
**CPE-1709**/**CPE-1461** (`is_safe_name`/`local_safe_segment`, the ADS shape at the transfer sink),
**CPE-1733** (the enumeration that first recorded this delta), **CPE-1759** (the other CPE-1744 remainder).

## Work Log (2026-08-17)

**Decision 1 — adopt, not reimplement.** `entry_name_is_safe` (`crates/server/src/archive.rs:1142`, now
`:1190` after the doc comment grew) now iterates every `std::path::Component` of the normalised name and,
for each `Component::Normal` segment, calls `crate::transfer::is_safe_name(seg)` and checks
`crate::transfer::local_safe_segment(seg)` returns the segment unchanged (`Cow::Borrowed`). `Component::CurDir`
(a lone `.`) still passes through untouched — unchanged from before, and needed to keep
`entry_name_is_safe("./x.txt") == true`. No third "is this leaf name safe" implementation was written;
both functions are reused byte-for-byte from `crate::transfer`, same as `guarded_join` already applies
them per segment.

**Decision 2 — REFUSE, not rename.** `local_safe_segment` *sanitises* at the transfer sink
(`guarded_join`) because that sink owns the destination name outright — a rewritten segment is still a
fresh, unclaimed leaf under a caller-chosen root. Archive extraction has no equivalent freedom: every one
of `entry_name_is_safe`'s ~10 call sites (rows 15/16/19/20's `File::create` guards, the two `extract_zip_encrypted`/
`extract_zip_archive_stream`-style "inner" checks at what were lines 843/876, `extract_archive_entry_any`,
`extract_plan::plan_extract`, plus a handful more) already treat `false` as "skip this entry, keep
extracting the rest" — that contract predates this ticket and this fix does not touch it, it only widens
what `false` catches. Switching to rename would mean growing the function from a predicate into a
name-transform and threading a renamed destination through every call site, including the two
`sevenz-rust` callbacks (rows 19/20), which receive `entry_dest` already built by a crate we don't
control — there is nowhere to splice a rename in before the fact without reimplementing decompression
details `sevenz-rust` gives us for free. That is exactly the "three implementations for one question"
sprawl the ticket warns against, for a ticket scoped to *what a name may be*. Skip is also not new
silence: `extract_plan::plan_extract` (the plan the user reviews *before* committing to extract) already
records every `entry_name_is_safe` rejection in `skipped_unsafe` — the least silent point in the whole
call graph — and the streamed extraction paths (rows 16/20) already record a per-entry note in
`ArchiveReport.errors`. The two *one-shot* paths (rows 15/19) stay silent on a skip, but that limitation
predates this ticket (the row-15 code comment already said so before this change) and is a
different-shaped gap — the return signature has nowhere to put a per-entry note — not something CPE-1758
was scoped to fix.

**extract_plan impact.** `extract_plan::plan_extract` (`crates/server/src/extract_plan.rs:106`) calls
`entry_name_is_safe(&entry.name)` and pushes the raw name to `skipped_unsafe` on `false` — no code change
needed there. The only effect is that more entries (ADS/colon names, `..evil`-shaped names, and on
Windows reserved-device/trailing-dot-space names) now show up in `skipped_unsafe` in the plan the
frontend shows before extraction, instead of silently being planned as if they'd extract safely. Ran
`cargo test extract_plan::` (9 tests) after the change — all green, none of its fixtures use the newly
widened shapes.

**Docs.** `src/docs/explorer-archives.md`'s zip-slip bullet described only the traversal half; added a
new bullet immediately after it ("Entry *names*, not just where they point, are checked too") describing
the colon/ADS, leading-`..`, and Windows reserved-device/trailing-dot-space shapes, and that they are
skipped (not renamed). This is an existing section, not a new one, so no `src/lib/sectionDocs.ts` entry
was needed (CPE-579 only requires that for a new section).

**Section comment + test re-pointed.** The `archive.rs` module-level comment recording the gap
(previously lines ~326–352) is updated in place: kept the "before" M7/M8 measurement, marked the gap
closed by CPE-1758, and added a "re-measured after" block. `entry_sink_action`'s doc comment (near line
586) and the CPE-1733 table's row-15/16/19/20 guard column were checked — the table cells already just
say `entry_name_is_safe`, so no wording change was needed there (the guard's name didn't change, only
its coverage). The test `archive::tests::entry_name_is_safe_accepts_shapes_transfers_is_safe_name_rejects`
(was line 2892) is **renamed, not deleted**, to `entry_name_is_safe_now_agrees_with_transfers_is_safe_name`
— its rows for `"file:stream"`, `"..evil"`, `"..:$DATA"` flip from `(true, false)` (disagreement) to
`(false, false)` (agreement); `"a/b.txt"` stays `(true, false)`, a deliberate, still-correct disagreement
(multi-segment paths are legal to `entry_name_is_safe`, illegal to `is_safe_name`, which only ever judges
one segment). A second, `#[cfg(windows)]`-gated test,
`entry_name_is_safe_rejects_windows_device_names_and_trailing_dot_space`, was added for the
`local_safe_segment`-only shapes (`con`, `CON`, `con.txt`, `nul`, `" sp "`, `"x."`, `"trailing "`), because
none of those appear in the `is_safe_name` comparison table (that function has no device-name or
trailing-run logic at all) and asserting them unconditionally would assert a Windows-only hazard as
universal, on a repo where "never assert Windows-only shapes unconditionally" is a hard rule (3-OS CI
matrix). Renamed rather than deleted per the ticket's explicit instruction.

**Red-proof (guard broken on its own, real output pasted, per the ticket's "distinct test, distinct red"
requirement):**

1. Disabled only the `is_safe_name` call (temporarily replaced it with the `local_safe_segment`-only
   check), ran `cargo test entry_name_is_safe_now_agrees`:
   ```
   test archive::tests::entry_name_is_safe_now_agrees_with_transfers_is_safe_name ... FAILED
   thread '...' panicked at src\archive.rs:2989:13:
   assertion `left == right` failed: archive::entry_name_is_safe("..evil") changed — if this un-does the
   CPE-1758 fix, update the table in this module's section comment (and src/docs/explorer-archives.md) too
     left: true
    right: false
   test result: FAILED. 0 passed; 1 failed; ...
   ```
   `"..evil"` has no colon, isn't a device name, and has no trailing dot/space, so `local_safe_segment`
   alone never catches it — confirms this row is testing `is_safe_name`'s reason, not just an effect two
   guards could both produce.
2. Restored `is_safe_name`, disabled only the `local_safe_segment` check (`#[cfg(any())]`-gated it out),
   ran `cargo test entry_name_is_safe_rejects_windows_device_names`:
   ```
   test archive::tests::entry_name_is_safe_rejects_windows_device_names_and_trailing_dot_space ... FAILED
   thread '...' panicked at src\archive.rs:3027:13:
   entry_name_is_safe("con") should be false on Windows — local_safe_segment would rewrite this segment,
   so it is one of the CPE-1758 shapes
   test result: FAILED. 0 passed; 1 failed; ...
   ```
   `"con"` passes `is_safe_name` (no colon, no leading `..`, single component), so only
   `local_safe_segment` catches it — confirms the second guard is load-bearing on its own.
3. Restored the real fix (`diff` against the pre-mutation backup showed zero difference), re-ran both
   tests plus the full `entry_name_is_safe` group — all green.

**Verification run (Windows 11, local box; `cargo` at `~/.cargo/bin`, not on PATH by default):**
- `cargo build -p cpe-server` (via `cd src-tauri`) — OK.
- `cargo test` (crates/server, default features) — **2199 passed, 0 failed, 4 ignored** (pre-existing
  ignores, unrelated to this ticket).
- `cargo test extract_plan::` — 9 passed, 0 failed.
- `cargo clippy --all-targets -- -D warnings` (default) — clean.
- `cargo clippy --all-targets --features index -- -D warnings` — clean; `cargo test --lib --features
  index` — **2247 passed, 0 failed, 4 ignored**.
- `cargo clippy --all-targets --features pdf-thumb,video-thumb,waveform,dicom-thumb -- -D warnings` —
  clean. **Did not run the full test suite under this feature combo** (native ffmpeg/pdfium-backed
  thumbnailing tests, several minutes, and this change touches none of that code — clippy already lints
  the test bodies under this feature set). Flagging as unverified-but-low-risk rather than silently
  skipping it.
- No `specta::Type` struct was touched (only `archive.rs` function bodies/doc comments and
  `src/docs/explorer-archives.md`), so `bindings.gen.ts` was not regenerated — nothing to regenerate.
- No Rust dependency changed, so `src-tauri/Cargo.lock` was not touched.
- `git diff --numstat` checked after every edit (all edits made via the `Edit` tool, not PowerShell) —
  no unexpected BOM/CRLF churn beyond git's existing LF→CRLF autocrlf warning on this file, which is
  pre-existing repo config, not introduced here.

**Assumptions / judgment calls:**
- Read "adopt … per segment" as reusing the two *predicates* (`is_safe_name`, and
  `local_safe_segment`'s rewrite-vs-unchanged signal used as a predicate), not literally splicing
  `guarded_join`'s join logic into `entry_name_is_safe` — `entry_name_is_safe` doesn't build a path, it
  only judges a whole `name: &str`, so there's no `base: &Path` to join onto.
- Kept `Component::CurDir` passing through unchecked (matches pre-existing behavior and the still-green
  `entry_name_is_safe_rejects_traversal` test's `"./x.txt"` case) rather than running it through
  `is_safe_name`, which would reject a bare `"."` — that would be a scope-creeping behavior change to
  traversal handling, which this ticket doesn't touch.
- Did not change the row-15/19 one-shot-extractor silent-skip limitation (no per-entry error slot in
  their `Result<String, String>` signature) — recorded above as pre-existing and out of scope; a future
  ticket could widen those signatures to `ArchiveReport`-shaped output if that silence is judged worth
  fixing on its own.

## Work Log addendum (2026-08-18) — review round 2 fixes

Two independent reviewers (Reviewer + Security Auditor) found four things in the first PR revision.
Addressed all four before re-pushing:

**Finding 1 (BLOCKER, real bug, mine) — the `Cow` proxy silently rejected every `%` name on Windows.**
`archive.rs`'s `entry_name_is_safe` matched `crate::transfer::local_safe_segment(seg)`'s return on its
`Cow::Borrowed`/`Cow::Owned` *variant* as a proxy for "would this segment be rewritten". But
`windows_safe_segment`'s pre-scan is *deliberately* over-broad for any segment containing a bare `%` (to
escape a pre-existing `%XX` its own encoder could have emitted) — it allocates an `Owned` copy even when
that copy is byte-identical to the input. That "allocated but identical" case is exactly what a
`Cow`-variant match cannot distinguish from a genuine rewrite. Effect: an ordinary Windows filename like
`"50% off.txt"`, `"report%2ffinal.txt"`, or a Hive/Athena partition value `"city=A%2FB"` (the literal
example CPE-1709 round 2 fixed the mangling of) was silently rejected as "unsafe", reproducing the exact
"successful-looking extraction, missing file" shape this ticket exists to eliminate — for a common
character, invisible on every CI leg (`local_safe_segment` is the identity on Linux/macOS; the pre-fix
Windows test table had no `%` row).

**Fix:** changed the comparison from a `Cow`-variant match to a content comparison —
`crate::transfer::local_safe_segment(seg).as_ref() != seg` (`archive.rs:1208` at the time of the fix; the
doc comment grew again since). Applied the same fix to the existing Windows device-name test's assertion.
Added a new `#[cfg(windows)]` regression test,
`entry_name_is_safe_does_not_reject_percent_names_that_round_trip_unchanged`, covering `"50% off.txt"`,
`"report%2ffinal.txt"`, `"city=A%2FB"`, `"100%"`, `"a%b"`, `"ok/50% off.txt"` — all now `true`. Re-ran the
existing intended-refusal rows (`file:stream`, `..evil`, `con`, `nul`, `" sp "`, `x.`) — all still `false`.
Full archive test module: 60 passed, 0 failed (was 58 before this ticket's first two tests, +2 more for
this round = 60).

**Finding 2 (BLOCKER, doc-only) — cited a surfacing route with zero callers.** My first Work Log entry
and the `archive.rs` section comment said the skip "lands in `skipped_unsafe`, which is the plan the user
reviews before committing to extract". Both reviewers independently confirmed by grep
(`skipped_unsafe|plan_extract|ExtractPlan` over `src/`, `src-tauri/src/`, `*.svelte`) that
`extract_plan::plan_extract` has **zero callers outside its own module** — it is not wired to a Tauri
command and there is no plan-review UI. Corrected both the `entry_name_is_safe` doc comment and the
section comment to name the *real* route instead: the streamed extractors (rows 16/20) push
`"{name}: unsafe entry name, skipped"` into `ArchiveReport::errors`, which reaches the user as an error
count in the operations panel (`TransferPanel.svelte`); `plan_extract`'s `skipped_unsafe` is mentioned
only as "also records it, though currently unconsumed", not as the primary surfacing route. Also learned
(and left as a note, not a fix, per the reviewer's instruction not to touch it in this PR):
`App.svelte`'s notice only fires on `r.failed > 0`, and an unsafe-name skip increments neither `failed`
nor `skipped`, so the toast still reads "Extracted" — only the operations-panel tooltip shows the skip.
This is exactly why the `explorer-archives.md` line telling the user to check the operations panel is
load-bearing, not decorative.

**Finding 3 — added the end-to-end test the ticket's own checklist demanded.** Both new predicate tests
from round 1 are pure `entry_name_is_safe(&str) -> bool` calls; neither touches a filesystem or a
`Result`. Added `ads_shaped_entry_is_skipped_end_to_end_and_recorded_not_silently_dropped`
(`#[cfg(windows)]`): builds a real ZIP containing `"file:stream"` + `"ok.txt"`, pre-creates a neighbour
file `"file"` in the destination, runs the real `extract_archive_streamed` (what `start_archive_extract`
calls for the shipping Extract button on `.zip`), and — **before unwrapping the `Result`** — asserts the
neighbour's bytes are untouched and no literal `"file:stream"` leaf exists; only then unwraps and asserts
`report.errors` actually contains the skip message and that `"ok.txt"` still extracted. That last
assertion is the one that would have caught Finding 2 on its own. Armed a `RemoveOnDrop` guard (new,
local to this test — no pre-existing Drop-guard idiom was found elsewhere in `archive.rs`'s tests to
reuse, so a minimal one was written) before any assertion, so a panicking assertion still cleans up the
scratch dir (CPE-1693).

**Finding 4 — two stale sentences fixed.** `archive.rs`'s "Rows 19–20 inherit this section unchanged"
paragraph said "the caveat below is theirs too: `entry_name_is_safe` is not `is_safe_name`" — reworded to
state the fix is now shared, not the old gap. The closing paragraph of the "three extractors" section said
"the `entry_name_is_safe`/`is_safe_name` delta is **CPE-1758**" as if still open — appended "(closed)".

**Security Auditor addendum — scope correction, not new work.** The auditor confirmed the guard fires
correctly at every sink this PR touches (`ok/file:stream` → `false`, verified) but found two sinks that
`entry_name_is_safe` is never called from at all: `extract_tar_stream`'s `entry.unpack_in` path
(`archive.rs`, used by `start_archive_extract` for `.tar`/`.tar.gz`/`.tgz` — the real Extract button) and
the one-shot `tar_unpack`/plain-ZIP-fallback `archive.extract(dest_path)` path. **Not fixed here** — the
auditor is filing it separately (their ticket numbers, not repeated here since this file shouldn't name a
ticket that doesn't exist yet in this queue) together with a demonstrated ZIP symlink-target escape and
the invisible-skip UX gap (`App.svelte`'s notice threshold, above). Per instruction, did not touch the
TAR path or the ZIP fallback in this PR. **What this DID change:** reworded
`src/docs/explorer-archives.md`'s new bullet so it names exactly which sinks are covered (ZIP streamed +
password-protected, 7-Zip both paths, the single-leaf open path) and explicitly states TAR extraction
does not run this check yet, plus added a closing sentence pointing at the open TAR gap, the ZIP
symlink-target escape, and the operations-panel skip-visibility gap as related open work — without
naming internal ticket IDs in the user-facing doc (matching this doc file's existing convention; grepped
`CPE-[0-9]` in it, zero pre-existing hits).

**Re-verification after all four fixes:** `cargo test --lib archive::` — 60 passed, 0 failed (default
features). `cargo test` (whole crate, default features) — 2201 passed, 0 failed, 4 ignored (was 2199
before this round's 2 new tests). `cargo clippy --all-targets -- -D warnings` — clean. `cargo clippy
--all-targets --features index -- -D warnings` — clean. `cargo clippy --all-targets --features
pdf-thumb,video-thumb,waveform,dicom-thumb -- -D warnings` — clean.
