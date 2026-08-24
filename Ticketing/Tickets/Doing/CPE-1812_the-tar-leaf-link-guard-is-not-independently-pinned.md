---
id: CPE-1812
title: the tar leaf-link guard is not independently pinned, so removing it leaves the whole suite green
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
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
