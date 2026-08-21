---
id: CPE-1814
title: archive.rs carries a dead Skip|Abort collapse, a staging failure that returns instead of continues, and dangling cfg-gated doc links
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

Four small defects in `crates/server/src/archive.rs`, all found while reviewing PR #958 and all deliberately
left out of it rather than widening a PR that already took three rework rounds:

1. **A dead `Skip(m) | Abort(m) => Some(m)` collapse at `:3186`.** Its only feeder returns `Write`/`Skip`,
   so the `Abort` arm is unreachable **today**. This is the *exact* construct that became CPE-1759's first
   blocking finding — it was dead there too, until a new feeder made it live and an entry started silently
   skipping where it should have aborted. It is a loaded gun with the safety on.
2. **A staging failure `return`s instead of `continue`s at `:6187`.** A fixture that fails to stage one
   entry abandons the rest of the setup, so the test runs against a **partially built** archive and passes,
   having exercised far less than its name claims. Third sighting of this shape in this file.
3. **Three dangling `cfg`-gated intra-doc links** — they resolve only on the platform whose item is
   compiled, so `cargo doc` on the other platform emits a broken link.
4. **A false comment at `:3081`** claiming a consolidated loop "is now the only zip extractor". See
   CPE-1807 — `extract_zip_encrypted` is a fourth, unmerged loop.
5. **An unqualified taxonomy entry at `:451-455`.** The decision-kind taxonomy CPE-1759 added as its own
   checkable rule still lists "a link this platform will not create" among refusals **without the ZIP/TAR
   qualification** the rest of that PR spent three rounds getting right. It is new in CPE-1759 and its
   final delta did not reach it. Non-blocking there because it is an internal comment rather than shipped
   help, it describes *kinds of decision* rather than a format promise, and the same comment block
   corrects it 33 lines down at `:488` — but a taxonomy that contradicts itself within one block is worth
   one line of work. **Fix it together with CPE-1813**, whose whole subject is that split.

## Why it matters

Item 1 is the interesting one and the reason this is `Medium`-worthy despite the `Low` priority: **a dead
arm is not a harmless arm.** CPE-1759 demonstrated the failure mode end to end within a single PR — the
construct sat inert on `main`, a change added a feeder, and it immediately produced `Ok` with an entry
missing. Removing it (or making it abort) costs nothing now and forecloses that.

Item 2 is the recurring one. It has now been found three separate times in this file, which suggests
copy-paste rather than coincidence — worth a sweep, not just three fixes.

## What to do

- For item 1: either delete the unreachable arm or make it abort. **Do not** leave it collapsed with a
  comment saying it is unreachable — that is precisely the state it was in before it became a bug.
- For item 2: **fail loudly** on a staging failure rather than `continue`; a fixture that cannot be staged
  is a broken test, not a smaller one. Then grep the file for every `return` inside a setup loop and report
  what the sweep found, even if it found nothing.
- Items 3 and 4 are text; fix by re-reading the code they describe.
- Red-proof item 2 by making a stage fail deliberately and confirming the test now fails rather than
  quietly shrinking.

## Notes

Filed by the Foreman from the round-2 and round-3 re-reviews of PR #958, 2026-08-20.

Related: **CPE-1759** (where the dead-collapse shape became a live bug), **CPE-1807** (the fourth zip loop),
**CPE-1809** (the earlier sighting of the staging-failure shape).

## Work Log

**2026-08-20 — Worker.** Branched off latest `main` (`5dcf421e`, already carrying CPE-1813). Re-read
CPE-1813's diff first — it added `recover_link_syscall_error`/`parse_os_error_code`/
`tar_link_creation_outcome` and brought TAR to parity with ZIP's link-creation refusal, but did **not**
touch any of this ticket's five items; none of them were stale from that merge. Line numbers in the
ticket (filed against PR #958's branch state) had drifted from CPE-1813's edits; I re-derived every one
against the current file rather than trusting the ticket's numbers, and pulled PR #958's own review
comments (`gh api repos/:owner/:repo/issues/958/comments`) to pin item 2's exact target, since two
candidate `return`-in-a-loop shapes existed in the file and only the review comments (specifically
"`archive.rs:6187` — a staging failure still `return`s out of the `for streamed` loop") disambiguated
which one.

1. **Dead `Skip|Abort` collapse — fixed.** `archive.rs:3444` (`extract_zip_archive_stream`'s symlink-target
   arm) had `EntrySlotAction::Skip(m) | EntrySlotAction::Abort(m) => Some(m)`, the identical construct
   CPE-1759 found and fixed in `tar_entry_refusal` after it went live once — dead today because
   `link_target_action` only ever returns `Write`/`Skip`. Split into an explicit `Skip` arm plus
   `Abort(e) => return Err(e)`, matching `tar_entry_refusal`'s own fix. No red-proof is possible: nothing
   feeds `Abort` today (confirmed identically true for `tar_entry_refusal`'s own fix per PR #958's round-4
   review — "It is dead... not a live defect"), so the change is defensive-only, foreclosing the CPE-1759
   failure mode rather than fixing a live one.
2. **Swallowed loop exit — fixed, at the location PR #958's own review named (`archive.rs:6187` at review
   time; `for streamed in [false, true]` inside
   `cpe1759_an_unreadable_slot_aborts_both_tar_paths_rather_than_being_skipped`).** Replaced
   `if !deny_stat_of(&slot) { skip_notice!(..); return; }` with `assert!(deny_stat_of(&slot), ..)` — fails
   loudly rather than `continue`ing (the ticket's explicit instruction), closing the gap for a *lenient
   local* run; `require_staged` inside `deny_stat_of` already fails loudly under CI (CPE-1717), so this
   makes the two consistent rather than duplicating machinery. Red-proofed: temporarily changed the
   `assert!`'s condition from `crate::fsutil::deny_stat_of(&slot)` to
   `false && crate::fsutil::deny_stat_of(&slot)` (one line), reran the single test, observed a panic at
   `archive.rs:7015` naming the staged path — RED — then reverted; confirmed green again after revert.
   **Swept the file for the same shape** (`return` after a staging check, inside a `for` loop over
   multiple rows/legs, abandoning the rest): found **8 more** occurrences —
   `every_guarded_row_refuses_a_live_link_without_touching_its_target` (`for (n, link_name, run) in
   GUARDED_ROWS`, 9 rows), `rows_15_and_16_refuse_a_live_link_and_still_extract_the_rest` (`for (n, label,
   run, records) in rows`), `rows_21_and_22_tar_refuse_a_link_at_an_entry_name_and_still_extract_the_rest`
   (`for (label, run, records) in legs`), the ZIP-alignment leg two functions later (`for (label, run,
   records) in legs`), `rows_15_to_20_refuse_a_file_entry_addressed_through_a_symlinked_intermediate_directory`
   and `row18_refuses_a_directory_entry_that_would_be_created_outside_the_extraction_folder` (both `for (n,
   label, kind, run[, records]) in sinks`, via `stage_intermediate_dir_escape`), and row 17's
   dangling-destination leg (`for (label, run) in legs`). **Left unfixed** — the ticket names one location
   and scope discipline says fix exactly that; mass-converting eight more call sites to `assert!` is a
   bigger, separate change (would also touch whether `require_staged`'s legitimate-skip semantics should
   still apply per-row) and belongs in its own ticket if wanted. Distinguished these from ~9 *other*
   `return`s after a staging check that are `#[test]`-scoped or precede the loop entirely (so nothing "the
   rest of" is abandoned) — those are fine as-is and not part of this shape.
3. **Dangling cfg-gated intra-doc links — fixed, and the count is exactly right.** Ran
   `cargo doc --no-deps --lib --document-private-items` on this (Windows) machine and got exactly three
   `warning: unresolved link to `EPERM`` at `archive.rs:1069`, `:1172`, `:1327` — `EPERM` is
   `#[cfg(unix)]`-gated but each referencing item (`link_creation_is_categorical`,
   `recover_link_syscall_error`, `materialise_entry_symlink`) is not gated at all, so the link resolves on
   unix and dangles on Windows — this machine — which is what fired here. The `WINDOWS_NO_LINK_SUPPORT`
   half of the same two sentences (`:1069`, `:1172`) plus one more
   (`recover_link_syscall_error`'s doc, `:1188` at review time) resolve fine on *this* platform (Windows)
   but dangle by the same logic on Linux/macOS, which I could not run `cargo doc` on to confirm directly —
   de-linked those three too so the fix is symmetric rather than only fixing the half visible from this
   machine. All four sites now spell the names as plain code text (backtick-only, no `[...]`) with a
   one-line note explaining why. Reran `cargo doc` after: zero `EPERM`/`WINDOWS_NO_LINK_SUPPORT` warnings
   remain.
4. **False "only zip extractor" comment — fixed.** `extract_zip_archive_stream`'s doc claimed to be "the
   only zip extractor" and "**every** zip extraction in this module" (both false: `extract_zip_encrypted`
   at `archive.rs:2387` is its own `for i in 0..archive.len()` loop, confirmed by reading it — CPE-1807 is
   the open ticket tracking that merge). Corrected both sentences and the section heading, with a note
   naming `extract_zip_encrypted` and CPE-1807.
5. **Taxonomy entry at `:451-455` — STALE, not fixed.** The ticket describes a *contradiction* that existed
   through PR #958's round 4 (pre-CPE-1813): the taxonomy calls "a link this platform will not create" an
   unqualified refusal, while the correction 33 lines down said that refusal was ZIP-only. **CPE-1813
   closed exactly this gap** — TAR now delivers the same refusal via `tar_link_creation_outcome`, so the
   correction paragraph (now `archive.rs:495-502`) no longer says "ZIP-only"; it says "CPE-1813 closed the
   gap" and that a tar link the volume cannot hold "now skips, the same as a zip one." The taxonomy line
   and its correction agree today — no code or comment change made. Reported per this ticket's own "even
   if it found nothing" instruction: this item's underlying inconsistency was resolved as a side effect of
   CPE-1813's behaviour change, not by anyone editing this comment block.

**Gates:** `cargo clippy --manifest-path crates/server/Cargo.toml --all-targets -- -D warnings` — clean, 0
warnings. `cargo test --manifest-path crates/server/Cargo.toml --lib` — `test result: ok. 2251 passed; 0
failed; 4 ignored; 0 measured; 0 filtered out`. Both run on Windows only (this machine); Linux/macOS legs
not run locally — left to CI's 3-OS matrix. `src-tauri`/`src/` untouched, so those gates don't apply.

PR: see the branch `cpe-1814-archive-cleanups`.
