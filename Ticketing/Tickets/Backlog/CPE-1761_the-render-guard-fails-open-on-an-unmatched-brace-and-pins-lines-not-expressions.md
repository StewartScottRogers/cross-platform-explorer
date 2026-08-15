---
id: CPE-1761
title: The render guard fails open on an unmatched brace, and pins line numbers rather than expressions
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-15
closed:
---

## Problem

Three blind spots found by the PR #918 (CPE-1757) round-2 review, which approved that PR and recommended
these as the follow-up. The guard is a real barrier now — it caught 16 of 18 probe shapes versus 3 before,
and it holds its own line — but a guard's failure modes matter more than its hit rate, and two of these are
**fail-open**.

### 1. An unmatched `{` fails OPEN and silently ends the scan for the rest of the file

`findMatchingBrace` returns `-1`, and `handleMustache` sets `i = markup.length` (`bidiRenderScan.ts:279`),
which terminates the scan of the whole file. Reproduced:

```
baseline-two-raw       <div title="x">  + 2 raw renders  -> [2,3]
brace-in-text          <p>use { as a brace</p> + same 2  -> []
unterminated-mustache  <div title="{">  + same 2         -> []
```

A lone `{` in ordinary prose — a perfectly reasonable thing to write — disables the guard for everything
below it and reports `[]`, which is **indistinguishable from "clean"**: the most reassuring output the tool
can produce. That is the worst possible failure direction for a guard whose entire purpose is to stop
people having to remember.

Partially self-defending: for the 40 files with non-empty recorded arrays, truncation drops recorded lines
and trips the STALE check. It is fully silent only when the trap sits below every recorded line, or in a
file recorded at `[]`.

**Fix:** treat `-1` as a hard error rather than end-of-scan. Two lines.

### 2. Same-line substitution defeats the equality check

Recorded entries pin **line numbers**, not expressions. `PreviewPane.svelte:1015` is a recorded offender
(`title={$t(action.labelKey)}` — harmless i18n). Editing that same line in place to `title={entry.name}` — a
genuinely raw filesystem name in a tooltip — leaves the computed set identical and the guard **green**.

With ~700 recorded lines across 41 files, every one is a slot where a raw name can be swapped in.

**Fix, mostly already built:** `RenderSite` (`bidiRenderScan.ts:225-230`) already carries `expr` alongside
`line`; `findUnsafeRenderLines` returns `number[]` and discards it. Record `line:expr` and compare that.

### 3. A stray `<` in text content suppresses renders until the next `>`

`<div>a < {entry.name} b</div>` → `[]`; the same render one line later is caught. Narrow window, same
STALE-check mitigation as #1, and the same fail-open direction.

## Also — two stale phrases in the shipped doc

`src/docs/03-explorer.md:92` still says "see the guard test's `ALLOWLIST`"; that constant is now `REGISTRY`.
`:83` calls it "a grep-based guard test", which round 2 is no longer — it is a parser. The parity test
covers neither phrase, which is why they drifted.

## Acceptance criteria

- [ ] An unmatched `{` (and an unmatched `<`) causes the guard to **fail loudly**, naming the file and the
      position — never to report an empty offender set.
- [ ] A test proves it: a file containing a lone `{` above a raw render must red, and the failure message
      must say the scan could not be completed rather than that the file is clean.
- [ ] Substituting a raw filesystem name into an already-recorded line reds the guard. Demonstrate on
      `PreviewPane.svelte:1015` specifically, since that is the measured instance.
- [ ] The recorded-set comparison stays readable — a developer seeing the failure must be able to tell which
      expression changed, not just that a hash differs.
- [ ] The two stale doc phrases are corrected, and consider whether the parity test can cover the wording it
      points at.
- [ ] The guard's header list of what it "still cannot see" is updated to match what remains true after this
      ticket.

## Notes

Related: CPE-1757 (PR #918 — the guard), CPE-1712 (the spoof fix it protects), CPE-1760 (the prop-pass-through
leaf, blind spot #4 in the same header).
