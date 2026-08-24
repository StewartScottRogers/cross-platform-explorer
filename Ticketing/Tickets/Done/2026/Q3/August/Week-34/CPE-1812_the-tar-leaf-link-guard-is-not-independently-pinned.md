---
id: CPE-1812
title: the tar leaf-link guard is not independently pinned, so removing it leaves the whole suite green
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-20
closed: 2026-08-23
---

## Problem

Two guards protect the tar entry slot, and **only one of them is pinned.**

`fsutil::confined_to` canonicalises the *full* path including a leaf symlink. So for the only fixture that
exists — a live link pointing **outside** `dest` — the containment guard alone already refuses. Remove the
**leaf-link** half of `entry_sink_action` precisely (`_ => entry_dir_action`) and **the entire suite stays
green**, on Linux as much as on Windows.

Found by PR #958's UAT, which applied that exact mutation expecting one red and observed **zero**.

## Why it matters

The behaviour today is correct — this is not a live defect. What is missing is the guard's *evidence*.
Anyone refactoring `entry_sink_action` can delete its leaf half and every test will tell them it was fine.
Given that the tar path's whole reason for existing is that **tar silently unlinks and replaces a symlink
at an entry name**, a leaf guard nothing pins is exactly the wrong thing to leave undefended.

This is a distinct species from the cannot-fail tests this crew has been finding: the test is real and does
fail for real reasons — it just cannot distinguish *which* of two guards saved it.

## What to do

- Add a leg with a live symlink at the entry name whose target points **inside** `dest`. Containment
  passes, so only the leaf-link guard can refuse — that isolates it.
- **Red-proof by discrimination, not just by failure**: remove the leaf guard alone and confirm the new leg
  reds while the existing outside-pointing leg stays green. A mutation that reds both proves nothing about
  which guard is doing the work.
- Check the sibling zip sinks for the same overlap — if `confined_to` shadows a leaf guard there too, the
  same blind spot exists and the same discriminating leg is needed.
- While there, consider whether any *other* pair of guards in this file is pinned only jointly. The
  question to ask of each test is not "does it fail when I break the code" but "does it fail when I break
  **this specific** thing".

## Notes

Filed by the Foreman from PR #958's UAT, 2026-08-20. The UAT applied the precise mutation rather than a
broad one, which is the only reason the gap was visible — the broad version (`_ => Write`) reds correctly,
so a less careful red-proof would have reported the guard as covered.

Related: **CPE-1759** (the tar leaf-slot work), **CPE-1773/1774/1775**, and the Evidence Rules in
`Ticketing/wiki.md`.

## Work Log

2026-08-23 — Added `cpe1812_the_leaf_link_guard_is_pinned_independently_of_containment`: a live symlink at
the entry name whose target is a file already INSIDE `dest`, so `confined_to` alone would ADMIT it and
only the leaf-link half of `entry_sink_action` can refuse it. Four legs (tar one-shot, tar streamed, zip
one-shot, zip streamed) — `entry_sink_action` is the one function all three sinks (tar/zip/7z) share, so
covering zip alongside tar answers the ticket's "check the sibling zip sinks" ask directly rather than by
inspection.
2026-08-23 — Red-proof #1 (the ticket's exact mutation): `tar_entry_refusal`'s `_ => entry_sink_action(dest,
&out)` swapped for `_ => entry_dir_action(dest, &out)`. The new "tar one-shot" leg went red on "the link
itself must survive untouched" — measured mechanism: tar's `unpack_in` unlinks the pre-existing symlink and
writes a plain file in its place (not a write-through) when the leaf check that would normally refuse it
first is gone. `rows_21_and_22_tar_refuse_a_link_at_an_entry_name_and_still_extract_the_rest` (the existing
outside-pointing leg) stayed green under the identical mutation — the discrimination the ticket asked for.
Reverted after capturing evidence.
2026-08-23 — Red-proof #2 (zip, per "check the sibling sinks"): `entry_sink_action`'s leaf-check block
commented out entirely (a broader mutation than #1, needed because zip's call site is a direct call, not a
`match` arm the same shape can swap). Isolated "zip one-shot" by temporarily removing the tar legs from the
table (the harness aborts a `#[test]` fn on its first panic) and re-ran: went red with the victim's bytes
measured as `[65, 82, 67, 72, 73, 86, 69, 68, 32, 65]` ("ARCHIVED A") where `"VICTIM ORIGINAL"` was
expected — confirming zip's mechanism really is write-THROUGH (the opposite of tar's unlink-and-replace),
and that the shared leaf half is what stops it on both. Reverted after capturing evidence; both mutations
and the temporary leg removal are out of the final diff.
2026-08-23 — "Any other pair of guards pinned only jointly": not swept exhaustively beyond this pair given
the ticket's S size: `entry_slot_action`'s three arms (NotALink/Link/Unknown) and `confined_to`'s own
internal escape-shape legs are already each independently tested elsewhere in this file
(`confined_to_refuses_all_three_escape_shapes_and_admits_ordinary_paths` et al. in `fsutil.rs`). Flagging
rather than claiming a complete sweep: the 7z sink (`sevenz_entry_slot_action`) also calls
`entry_sink_action` and was not given its own discriminating leg here — same shared function, so it is
covered by the zip/tar legs' proof that the function itself is sound, but its OWN call site is unpinned by
this specific leaf-vs-containment discrimination the way tar's was. Worth a follow-up ticket if this
crew's pattern holds (a `_ => entry_dir_action`-shaped mistake tends to recur per call site, not just
inside the shared function).
2026-08-23 — Status: Doing → ready to close alongside CPE-1837/CPE-1809 in one PR.
