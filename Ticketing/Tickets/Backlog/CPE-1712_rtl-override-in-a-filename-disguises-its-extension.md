---
id: CPE-1712
title: A right-to-left override in a remote filename disguises its extension in Explorer
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-13
closed:
---

## Problem

Found by the PR #894 (CPE-1709) UAT, 2026-08-13, while enumerating character classes the download sink
mishandles. **Pre-existing — not introduced by that PR**, which is why it was filed rather than folded in.

`char::is_control()` matches only the **Cc** Unicode category. `U+202E RIGHT-TO-LEFT OVERRIDE` is category
**Cf** (format), so it passes every guard untouched.

Measured: a remote leaf `\u{202E}gnp.txt` downloads successfully and lands as a real file that **Windows
Explorer displays as `txt.png`**. The bytes are intact and the name is legal; only its *rendering* lies.

## Why it matters

This is the classic filename-spoofing trick, and a file explorer is precisely the application where it
lands. A user looking at a downloaded listing sees what appears to be a PNG and double-clicks it. What runs
is whatever the real extension says.

It is a **display** problem rather than a data-loss one, which is what separates it from CPE-1709 — the
bytes arrive correctly and the file opens. That is also why it is Medium rather than High: nothing is lost,
and the user must still act on the misrepresentation.

## Scope

The same sink CPE-1709 touched — `crates/server/src/transfer.rs`'s leaf handling — plus, importantly, **our
own rendering**. We control how the explorer draws a name; Explorer's behaviour is not ours to fix, but the
app showing the same lie is.

## The decision to make, and record

There are two distinct questions and they may get different answers:

1. **On disk.** Should a bidi control character be escaped in the local name the way CPE-1709 escapes
   Windows-unholdable characters? That keeps the file honest everywhere, at the cost of altering names that
   are legitimate in genuinely right-to-left languages. **Do not casually mangle real RTL filenames** —
   Arabic and Hebrew names contain legitimate bidi marks, and an over-eager rule would make the app
   unusable for those users while fixing a spoof they never encounter.
2. **In our UI.** Should the explorer render bidi controls visibly (an escape, a badge, an isolate) so a
   spoofed name cannot masquerade in *our* list even if it does in Explorer? This is likely the better
   half of the answer, because it is where we can act without touching anyone's data.

The full Cf set is worth considering, not only `U+202E`: `U+202A`–`U+202E`, `U+2066`–`U+2069`, and
`U+200E`/`U+200F`.

## Acceptance criteria

- [ ] A remote name containing `U+202E` cannot present a misleading extension **in this app's own listing**.
      Record what it does show.
- [ ] Decide and record whether the on-disk name is transformed. If it is, legitimate RTL filenames must
      survive — test with real Arabic and Hebrew names, not only with the spoof.
- [ ] If the on-disk name is transformed, the mapping stays **injective** and round-trips, per CPE-1709's
      construction. Two distinct remote names must never collide onto one local file.
- [ ] Enumerate the bidi/format set rather than fixing `U+202E` alone — the same "fix the reported
      character only" trap CPE-1709 explicitly avoided.
- [ ] A test proves it, asserting on what the user sees, and breaking the guard turns a **distinct** test
      red, per the Evidence Rules in `Ticketing/wiki.md`.
- [ ] Confirm no regression in CPE-1709's encoder: ordinary names, percent-bearing names, and the hostile
      set it covers must be unaffected.

## Notes

Filed by the Foreman from the PR #894 UAT, 2026-08-13. The UAT correctly scoped it as pre-existing and out
of that PR's scope.

Related: **CPE-1709** (the sink and its encoder), **CPE-1704** (the listing guard that stopped imposing
filesystem rules on every backend).

## Fold in while you are in this file (from the PR #894 UAT, 2026-08-13)

`cpe_1709_a_security_refusal_still_reports_ok` exercises only the **traversal** branch. It is not a
happy-path assertion -- it mixes refused and deliverable entries and pins `n == 1`, so it does
distinguish the two categories -- but its own doc comment lists **three** security refusals (traversal,
pre-existing symlink, uninspectable ancestor) and only one is covered.

**A future change that moved `LeafProbe::PreExistingSymlink` into `undelivered` would pass this test.**
Add the missing cases when you next touch `crates/server/src/transfer.rs`.

Also worth recording there, deliberate as far as the UAT could tell but unstated: an **uninspectable
ancestor** ends `Ok` while an **uninspectable leaf** ends `Err`. That asymmetry is defensible -- the leaf
is the delivery target and the user genuinely did not get their file -- but it is not spelled out, and a
permission-denied leaf `lstat` now fails the whole transfer where it used to be silent. Judged an
improvement, not a defect; say so in the code rather than leaving it to be rediscovered.

One scoped limitation of CPE-1709 to note in passing: on an **astral-plane** name (emoji, where char
count and UTF-16 count diverge 1:2) the length explanation is **absent**, not wrong -- the message
degrades to the raw `os error 123`. Both properties that matter still hold: it ends `Err`, and it never
says "symlink".
