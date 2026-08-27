---
id: CPE-1912
title: a junction inside the backup destination silently redirects a whole subtree, and every entry still reports success
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-26
---

## Summary

Plant a directory junction `dst/Photos → dst/Trash` inside the backup destination. Nothing else. **No
race, no thread, no timing window.** Then back up.

    OpResult { path: "...\dst\Photos/holiday.jpg", ok: true, error: "", outcome: Applied }
    dst/Trash/holiday.jpg = "THE PHOTO THE USER IS BACKING UP"

The user's photos are written into `Trash`. `dst/Photos` keeps whatever stale content it had. **Every
entry reports success.** On restore, the files are silently missing or old.

100% reproducible, deterministic, measured in a single trial.

**Why every existing guard admits it.** Both the pre-write check (`parent_contained` →
`confined_to_resolved_root`) and CPE-1896's post-write check (`landed_inside`) compare the resolved path
only against the **destination root**. `dst/Trash` is inside the root. So both correctly conclude the
write stayed inside the destination — and it did. What neither asks is whether the bytes landed at the
path *the plan actually named*.

**Precondition:** write access to the destination tree. Strictly weaker than CPE-1896's race, which
needs the same access *plus* winning a timing window.

## Why this is not a defect in CPE-1889 or CPE-1896

Both of those guards do exactly what they say. Their contract is "the write stayed inside the backup
destination", and it did. This ticket is the observation that **that contract is not the property the
user cares about** — they care that `Photos` went to `Photos`.

CPE-1896 is simply the first code that could plausibly have caught it, because it is the first check
that runs *after* the write and knows where the bytes actually landed. It doesn't catch it, and it was
not asked to.

## Acceptance criteria

- [ ] Compare the landed path against the **plan's own expected resolution**, not only against the root.
      A write named `dst/Photos/holiday.jpg` that resolves to `dst/Trash/holiday.jpg` must be refused
      even though both are inside the root.
- [ ] Consider the handle-identity approach instead, which closes this and CPE-1896's swap-back window
      together: `copy_file_onto_no_follow_with_wording` already holds the destination handle and calls
      `batch_media::handle_facts(&w)` (`fsutil.rs:1456`), which yields file identity. Comparing identities
      rather than paths answers "did the bytes go where we meant" in one question. Decide between the two
      and record why.
- [ ] Red-proof it with the deterministic fixture above — no race needed, so this test can be a plain
      unit test that runs on every `cargo test`, not an `#[ignore]`d probe.
- [ ] Decide what a *legitimate* junction inside a backup destination should do. A user who deliberately
      links a folder into their backup tree may have meant it. Refuse, follow, or follow-with-a-notice —
      record the decision at the site. Refusing outright may be wrong; unlike the attacker case, the user
      chose the destination.
- [ ] Check whether the restore leg has the mirror of this problem: if a backup was written through a
      junction, does restore put the files back where the user expects?

## Notes

Filed 2026-08-26 by CPE-1896's independent Security Auditor, which found it while auditing that PR and
classified it as a separate ticket rather than a defect in the PR under review.

Related: **CPE-1896** (the swap-back race and the landing check this would extend), **CPE-1889** (the
static parent-containment guard), **CPE-1913** (the same silent-success shape across four other
subsystems), **CPE-1898** (the source leg has no containment assertion either).

Worth reading CPE-1896's audit before starting: the handle-identity approach is described there in
enough detail to implement, including why the hard-link route to forging an identity match is already
refused by the existing `facts.links > 1` branch at `fsutil.rs:1494`.

## Closed 2026-08-27 — fixed by CPE-1896, verified by CPE-1913's worker

**Not fixed by its own ticket. Closed because CPE-1896 made it unreachable, and then someone checked
rather than assumed.**

CPE-1913's worker was asked to determine whether this ticket was subsumed **by testing, not by
reading**, and it did:

- **Green on current `main`:** a junction `dst/Photos -> dst/Trash` planted inside the destination now
  gives `ok: false` with a refusal naming the component — *the path component "Photos" is a link (a
  symlink, junction or other reparse point)* — and nothing lands in `dst/Trash`.
- **Red with the pre-CPE-1896 open substituted** (`batch_media::open_no_follow` in place of
  `create_beneath`), reproducing this ticket's report **byte for byte**:
  `OpResult { ok: true, error: "", outcome: Applied }` with `trash_has_photo = true`.

**Why:** CPE-1896's per-component walk refuses a name surrogate at **every** component and never asks
where the path ends up — so "both paths are inside the root", which is what made this case slip
through before, stopped being a question the code asks.

AC4 is answered (refuse, naming the component) and AC5 holds by construction — Restore is the same
walk with the roots swapped.

**The fixture is now a permanent test** rather than a closed ticket's anecdote:
`backup::tests::cpe_1912_a_junction_inside_the_destination_never_silently_redirects_a_subtree`, landed
in PR #1050, with a liveness write so an inert fixture cannot pass.
