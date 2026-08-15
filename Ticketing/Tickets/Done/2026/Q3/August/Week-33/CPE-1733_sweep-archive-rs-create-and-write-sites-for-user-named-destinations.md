---
id: CPE-1733
title: Sweep archive.rs's ~14 create/write sites and decide which destinations are user-named
type: task
priority: Medium
status: Done
tags: ready
estimate: L
created: 2026-08-14
closed: 2026-08-15
---

## The observation, scoped exactly as it was given

From the PR #901 review, and note how carefully it is stated:

> `archive.rs` has ~14 `File::create` / `fs::write` calls at extract and archive-creation destinations with
> no clobber or link guard on any of them. **I did not verify which of those are user-named versus
> app-internal, so I am scoping the claim to "same primitive shape, unguarded" rather than "same bug".**

That scoping is the ticket. **The first job is not fixing anything — it is finding out which of the ~14
destinations a user actually names.** An extract destination is user-named almost by definition; a
temp-during-archive-creation is not. Nobody has done that split, and the fix for each half differs.

## Why this is worth doing

CPE-1710 → CPE-1719 and CPE-1718 → CPE-1729 both demonstrated the same thing: **the site a sweep declines
is the one the next round finds.** `archive.rs` is the largest remaining concentration of the primitive
this sprint has now guarded in four separate modules, and it has been declined by every sweep so far
because it was never in scope.

The primitives matter differently, and this sprint has measured all three:

- `File::create` / `fs::write` **follow a link and write through it** — the user's unrelated file is
  overwritten, the link survives, and the call returns `Ok` (CPE-1719, measured).
- `create_dir_all` is **not** destructive and on a dangling link does nothing at all (CPE-1729, measured
  after the opposite was assumed).
- Extraction adds a shape none of the earlier tickets had: **the archive controls the entry names.** That
  is a traversal question as much as a link question, and `cpe_server::transfer::guarded_join` /
  `is_safe_name` already exist for exactly it (CPE-1461).

## What to do

- [ ] **Enumerate first, fix second.** For each `File::create` / `fs::write` / `OpenOptions` site in
      `archive.rs`, record: the destination's provenance (user-named, app-owned, archive-controlled), the
      primitive, and whether a link at that path can reach a user's file. Publish the table before writing
      a guard.
- [ ] For user-named slots, `fsutil::create_slot_refusal` already exists (CPE-1718) and its link-first
      ordering is deliberate — read its doc before reusing it, and note it is **not** enforced by clippy
      (that decision is **CPE-1732**).
- [ ] For archive-controlled entry names, the question is traversal *and* link: an entry named `../x` and
      an entry landing on a link are different failures. Check whether `guarded_join` is already in the
      path; if it is, say so rather than adding a second guard.
- [ ] **Record the absences too.** If a site is genuinely app-owned, write that down at the site. CPE-1718
      established that an unrecorded absence is indistinguishable from an overlooked one.
- [ ] Each guard broken **on its own** turns a **distinct** test red, with real output pasted, per the
      Evidence Rules in `Ticketing/wiki.md` — and note **a red which is not the red you aimed at proves
      nothing**, which cost three people time this sprint.
- [ ] **Assert on the victim's bytes and on `symlink_metadata`, never on the returned `Result`.** Every bug
      in this family returned `Ok` while destroying something.
- [ ] Platform-gate correctly: a live **file** symlink cannot be staged on an unprivileged Windows runner —
      a junction is directory-only and a hard link is `is_symlink() == false` (CPE-1716). Use the
      pure-classifier split so something is covered everywhere, and `require_staged` so a runner that
      *should* be able to stage goes red rather than skipping (CPE-1717).

## Notes

Filed by the Foreman from the PR #901 review, 2026-08-14, deliberately **split** from CPE-1732 on that
reviewer's recommendation: this is investigation, that is a policy decision, and bundling makes the cheap
decisive one wait on the expensive exploratory one.

**L rather than M because the enumeration is the bulk of it.** If the table comes back showing every
destination is app-owned, the right outcome is to write that down and close — that is a success, not a
shortfall.

Related: **CPE-1718** (`create_slot_refusal` and the four-primitive sweep pattern), **CPE-1719**
(`fs::write` writes *through* a link — measured), **CPE-1729** (`create_dir_all` does not — measured after
the opposite was assumed), **CPE-1461** (`guarded_join` for archive-controlled names), **CPE-1732** (the
enforcement decision).

## Work Log

**2026-08-14 — enumeration published, then guards written from it.**

The table is in `crates/server/src/archive.rs`'s "Archive creation & extraction" section comment (17 rows).
Split:

- **Rows 1–5 — app-owned.** Every single-entry extractor (`extract_archive_entry`, `extract_tar_entry`,
  `extract_7z_entry`, `extract_rar_entry`, and `temp_extract_target` itself) writes under a per-call
  `%TEMP%/cpe-archive/<pid>-<seq>/` directory that this code creates and never reuses, with
  `Path::file_name()` as the leaf. Unreachable by the hazard, not merely unlikely. Recorded at each site.
- **Rows 6–12 — caller-supplied `dest`.** Six archive-creation destinations (`compress_to_zip`,
  `compress_to_targz`, `compress_to_zip_encrypted` and their three streamed siblings) plus
  `create_empty_zip`. Guarded.
- **Rows 13–14 — user-named folder, archive-derived leaf.** The `.gz` branch of `extract_archive` and
  `extract_archive_streamed`. Guarded.
- **Rows 15–16 — archive-controlled names under a user-named folder.** The two ZIP write loops. Guarded,
  but as a per-entry **skip** rather than an abort, matching the existing zip-slip skip.
- **Row 17 — the four `create_dir_all` roots.** Unguarded, deliberately: CPE-1729 measured that
  `create_dir_all` is not destructive and does nothing at all on a dangling link.

**Measured on Windows for this ticket** (not reasoned about): `File::create` on a dangling link → `Ok`,
creates the link's target; on a live link → `Ok`, target reads `"CLOBBERED"`, link survives both times.
`create_new` on either → `Err AlreadyExists (os error 80)`, target untouched. `File::create` on a dangling
**junction** → `Err Access is denied (os error 5)`.

That last one is why every leg pins the substring `"is a link"` rather than `is_err()`: a dangling junction
is the unprivileged-Windows staging fallback, so an `is_err()`-only leg would pass through a deleted guard.
Demonstrated live — with row 7's guard removed the test still went red, but on the *message* assertion,
reporting `Got: The file exists. (os error 80)`.

The `create_new` finding also downgrades row 7 from a safety fix to a **message** fix, and it is labelled
that way at the site.

**Traversal:** `guarded_join` is not in this path and was not added — `entry_name_is_safe` (CPE-628) is this
module's equivalent and is already applied at every site it writes itself. Said so rather than adding a
second guard, per the ticket.

**Recorded gap:** `tar::Archive::unpack`/`Entry::unpack_in`, `zip::ZipArchive::extract` and
`sevenz_rust::default_entry_extract_fn` create their files inside those crates, so a link already sitting in
the destination is still followed on the tar / 7z / one-shot-zip paths. Written down at the site and in the
in-app docs rather than left implied.

Guard: `fsutil::create_slot_link_refusal` — the link half of `create_slot_refusal` split out, one shared
implementation. The occupancy half is deliberately not applied; overwriting an existing archive is
long-standing behaviour of these functions.

Evidence: all 11 guarded sites neutralised **one at a time**, each turning its own named test red, restored
with `git checkout --` each time. Full `cpe-server` suite (2156 + integration) and `src-tauri` (182) green;
`cargo clippy --all-targets -D warnings` clean in both feature modes and in `src-tauri`.

**2026-08-14 (round 2) — PR #906 review (3 blockers) + UAT (6 findings). All nine reproduced here before
being acted on; every fix is in the enumeration, which is what this ticket delivers.**

Corrections to claims this ticket had made wrongly:

- **Rows 1–5 were not "unreachable by the hazard".** `create_dir_all` accepts a pre-existing directory
  *and* a directory symlink, so it established nothing; what protected them was `%TEMP%` being per-user on
  Windows. On a shared `/tmp` it is CWE-377 → CWE-59, and the leaf name is archive-controlled.
  **Hardened**, not deferred: `temp_extract_target` link-checks the shared root and claims its
  per-extraction directory with an exclusive `fs::create_dir` (`Err(AlreadyExists)` for both squat shapes,
  measured), retrying the next sequence number. Residuals stated at the site.
- **"still followed on the tar, 7z and one-shot-zip paths" was false for two of the three.** tar
  **destroys** the link (writes a regular file over it, silently, returning `Ok`); one-shot zip **aborts**
  the whole extraction; only 7z follows. The claim was inference that reached four places including the
  user-facing docs — the exact defect this ticket exists to stop.
- **`create_dir_all` on a dangling link does not "do nothing at all" — it fails**, with the same
  misleading "already exists" wording row 7 got a guard for. Row 17's rationale rewritten to one that
  survives comparison with row 7: `dest` is a folder the user *pointed at*, not a name being *claimed*.
- **`guarded_join` "not needed" was wider than the search.** True for traversal only; `is_safe_name`
  additionally fails closed on `:` and a leading `..`, and `entry_name_is_safe` accepts `file:stream`
  (NTFS ADS — bytes vanish, no visible file), `..evil`, `con`, `" sp "`, `x.`.
- **The rows 15/16 guard is LEAF-ONLY** — `create_dir_all(parent)` follows a directory link (a junction
  needs no privilege), and the leaf guard then sees nothing because the leaf does not exist.
- **Rows 15/16 collapsed "could not check" into "confirmed link"**, dropping an entry silently and
  returning `Ok`. Split into `fsutil::CreateSlotLink` + `archive::entry_slot_action`: a link skips, an
  unreadable slot aborts. Pure classifier, because the `Unknown` arm cannot be staged everywhere.

Non-blocking items taken: row 18 added (four per-entry `create_dir_all` calls the table omitted while
billing itself as the inventory) with a count line reconciling to the source; platform boundaries added to
the two figures lacking one; the live-link leg now states it walks rows 6–14 only.

Filed: **CPE-1746** (7z write-through, live on the shipping path — its own ticket, High),
**CPE-1744** (the ADS delta, the leaf-only escape, tar destroying links, the one-shot/streamed ZIP
divergence, row 17's wording), **CPE-1745** (`note_app_op` records an archive temp path never written).

New guard evidence, each broken on its own and restored with `git checkout --`: row 1's exclusive create
(victim overwritten with `"ARCHIVED A"`, `Ok` returned with an ordinary-looking path) and F6's three-way
action (`left: Skip("could not check")` vs `right: Abort(...)`). The recorded-absence test was also proved
un-rottable: simulating the CPE-1744 fix without updating the table reds it.

**2026-08-14 (round 3) — the two findings round 2 answered with prose instead of coverage.**

Round 2 corrected the tar/one-shot-zip/7z claims and then declined to pin them ("pinning behaviour we
consider wrong makes it harder to change"), and answered "rows 15/16 have no live-link leg" by writing
down that they had none. Both answers left the finding standing: the sentence that was wrong in two of
three cases was itself prose, and it survived four commits into the user-facing docs. Prose is not
cheaper to keep true — only cheaper to leave false. So:

- **The three extractors this module does not write itself are now characterization-tested**, one test
  each, each naming the ticket allowed to change it and the other places that must move in the same
  commit: `tar_extraction_destroys_a_link_at_an_entry_name_rather_than_following_it` (both tar paths —
  link replaced by a regular file holding the entry's bytes, victim intact, `Ok` returned),
  `one_shot_zip_extraction_aborts_everything_when_an_entry_lands_on_a_link` (`b.txt` absent is the
  assertion that separates "skipped an entry" from "abandoned the run"; the error must be the zip crate's
  *symlink* refusal, since `is_err()` alone would stay green through any I/O failure), and
  `sevenz_extraction_still_writes_through_a_link_until_cpe_1746` (asserts the live hazard, so CPE-1746's
  fix reds at the line describing the old behaviour rather than silently drifting from four descriptions
  of it).
- **Rows 15–16 have a live-link leg**: `rows_15_and_16_refuse_a_live_link_and_still_extract_the_rest`.
  The dangling legs cannot show what the missing guard costs — a dangling link has no bytes to lose. The
  victim is asserted **before** the `Result` is unwrapped, because these bugs return `Ok`. Broken on its
  own by making `CreateSlotLink::Link` write anyway:

  ```text
  row 15 (extract_zip_encrypted): the entry's bytes went THROUGH the link and truncated a file outside
  the destination that nobody named (outcome was Ok([]))
    left:  [65, 82, 67, ...]   ("ARCHIVED A")
    right: [86, 73, 67, ...]   ("VICTIM ORIGINAL")
  ```

Verification: `crates/server` 2163 unit (default) / 2265 (`--all-features`) + all integration green;
clippy `--all-targets -D warnings` clean in both feature modes and in `src-tauri`.

**2026-08-15 (round 4) — UAT PASS, reviewer CHANGES REQUESTED: four named fixes.**

Two of them were guards that could be deleted with the suite staying green — the same defect this ticket
keeps finding, now in its own tests.

- **NEW-4: finding 8's guard was untested where it mattered.** `an_unreadable_entry_slot_aborts_...`
  asserts `entry_slot_action`, which only *re-labels* an already-classified verdict; the decision UAT
  finding 6 was about — *is this `Err` a link or an unreadable slot?* — lived inside
  `create_slot_link_verdict`, next to the I/O. The reviewer flipped `_ => Unknown` to `_ => Link`
  (reinstating the bug exactly) and `cargo test --all-features` still reported 2265 passed. The
  pure-classifier argument was right and applied one level too low: split out
  `fsutil::create_slot_link_from_stat(&Result<bool>, &Path)`, leaving `create_slot_link_verdict` as one
  `symlink_metadata` plus a call, and pinned it with `an_unreadable_slot_is_unknown_never_a_confirmed_link`.
  Same mutation now:

  ```text
  assertion `left == right` failed: an lstat that failed with PermissionDenied is an I/O FAILURE, not a
  confirmed link. Classifying it as `Link` reinstates CPE-1733's UAT finding 6 exactly ...
    left: "Link"   right: "Unknown"
  ```

- **NEW-3: row 1's leg covered nothing about two runs in three, and said the opposite.** It predicted the
  next `EXTRACT_SEQ` value to squat, but that counter is shared with every sibling test that extracts, and
  cargo runs them in parallel; when the squat was missed, `!landed.starts_with(&squat)` was trivially true
  and the test passed silently. Its doc claimed the leg "announces rather than passing quietly" — **there
  was no announce mechanism**. Now it occupies a contiguous block of 64 names (far under
  `TEMP_TARGET_ATTEMPTS` = 1024), plants the link in every directory it actually created, and reads the
  landing sequence number: inside the block ⇒ fail, exactly at the end ⇒ walk proven, past the end ⇒
  nothing proven, retry, and only then a real `skip_notice!`. With `create_dir` → `create_dir_all`, three
  runs of the module were **3 of 3 red** (previously 1 of 3), always at the victim-bytes assertion.

- **NEW-2:** CPE-1744 still told its future worker the tar/one-shot-ZIP behaviours were "deliberately not
  pinned by tests" — round 2's stance, reversed in round 3. Corrected, both test names given, and its
  "What to do" list now says to re-point them (a false statement about coverage, inside the ticket family
  whose thesis is that an unpinned description is one nobody keeps true).

- **NEW-1:** `archive.rs:332` cited a test name that does not exist
  (`..._local_safe_segment_rejects` → `entry_name_is_safe_accepts_shapes_transfers_is_safe_name_rejects`).

Also folded in, as ticket text only: CPE-1746's option 1 rewritten to the **pre-call check in the existing
callback** the reviewer measured (`Ok(ArchiveReport { done: 1, errors: [..] })`, victim intact) with the
estimate revised M → S, and its checklist widened to `extract_7z_safe` (the one-shot path, also a
registered Tauri command); CPE-1744 given the two UAT addenda — the docs' "still follows a folder
shortcut" sentence is false for TAR, and `create_empty_zip` onto a plain existing file still returns a
bare `"The file exists. (os error 80)"`.
