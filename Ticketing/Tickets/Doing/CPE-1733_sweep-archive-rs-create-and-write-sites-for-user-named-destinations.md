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
