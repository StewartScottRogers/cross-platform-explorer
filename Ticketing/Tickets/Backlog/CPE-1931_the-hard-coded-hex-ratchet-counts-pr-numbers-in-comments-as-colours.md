---
id: CPE-1931
title: the hard-coded-hex ratchet counts `#1044`-style PR references in comments as colours
type: bug
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

`src/app.css.test.ts`'s CPE-1534 ratchet matches `#[0-9a-fA-F]{3,8}\b` anywhere in a `.svelte` file —
**including inside comments**. A ticket or PR reference whose digits are all valid hex characters is
counted as a hard-coded colour.

Observed 2026-08-27 on PR #1044: the ratchet went red at **401 vs a baseline of 399**, and the two
"new colours" were two comments reading `PR #1044 review round 2`. `1044` is valid hex. No CSS colour
was added anywhere in that PR — confirmed by diffing every added `.svelte` line against `main`.

## Why it matters more than the false positive itself

1. **It sent a Foreman instruction in the wrong direction.** I told the worker it had added two
   hard-coded colours and to replace them with tokens. It investigated instead of complying, found
   the real cause, and fixed it correctly. A less careful pass would have hunted for a colour that
   was not there, or — worse — raised the baseline to make the red go away, which is precisely the
   silencing move a one-way ratchet exists to prevent.
2. **It gets worse over time.** This repo's ticket numbers are now in the CPE-1900s, and every one
   of `#1900`–`#1999`, `#1044`, `#1892`, `#1234`… is valid hex. Comments citing tickets are
   *encouraged* here — most guards carry a `CPE-NNNN` rationale. So the false-positive rate rises as
   the codebase documents itself better, which is exactly backwards.
3. **The workaround pollutes the comments.** The immediate fix was to write `PR 1044` without the
   `#`, which makes the reference less scannable and, being invisible in intent, invites the next
   person to "restore" it and re-red CI.

## Acceptance criteria

- [ ] Strip comments before counting. `.svelte` files carry `//`, `/* */` and `<!-- -->`; the repo
      already has `stripShellComment`/`logicalLines` in `src/lib/preview/shellScriptLines.ts` as
      precedent for the "strip before matching" shape, though the languages differ.
- [ ] Prefer matching where a colour can actually appear — a `#hex` in a CSS value position — over
      matching it anywhere in the file. A regex that only fires inside `<style>` blocks and inline
      `style=` attributes would remove this whole class of false positive.
- [ ] **Re-baseline deliberately after the fix, once, and say so.** The current 399 was accumulated
      with false positives in it, so the true count is lower. Recount from scratch rather than
      subtracting the two we happen to know about — CPE-1922 is open on exactly this failure mode in
      the manual-test burndown, where a running total was patched forward instead of counted.
- [ ] Red-proof both directions: a genuinely new hard-coded colour must still fail, and a comment
      citing `#1044` must not.
- [ ] Sweep for the same shape in the repo's other content-scanning guards — anything matching a
      short hex-ish or numeric pattern across a whole file rather than in a syntactic position.

## Notes

Filed 2026-08-27 by the sprint Foreman after PR #1044's CI failure turned out to be a guard defect
rather than a code defect. Credit to that worker for checking rather than complying.

Related: **CPE-1534** (the ratchet), **CPE-1922** (a running total patched forward instead of
recounted), **CPE-1929** (guards that do not measure what they appear to).
