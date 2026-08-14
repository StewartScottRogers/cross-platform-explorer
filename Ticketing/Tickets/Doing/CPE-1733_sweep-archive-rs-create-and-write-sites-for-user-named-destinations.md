---
id: CPE-1733
title: Sweep archive.rs's ~14 create/write sites and decide which destinations are user-named
type: task
priority: Medium
status: In Progress
tags: ready
estimate: L
created: 2026-08-14
closed:
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
