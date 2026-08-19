---
id: CPE-1767
title: The render guard hard-fails CI on an apostrophe in a JS comment, and tells the developer to fix a brace that is fine
type: bug
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-17
closed:
---

## Problem

Found by the **PR #925 (CPE-1761) review**. CPE-1761 made `src/lib/bidiRenderScan.ts` fail *closed* on
markup it cannot parse — correct, and the right direction. This is the cost of that trade, and it is now
a **false positive that hard-fails CI on valid Svelte**.

`findMatchingBrace` has no concept of `//` or `/* */` comments, so an apostrophe inside a JS comment within
an inline tag-attribute expression reads as an opening string quote. The quote never closes, the brace never
matches, and post-CPE-1761 that is a thrown `RenderScanError`.

```svelte
<div on:click={() => { /* it's fine */ }}>…</div>
```

Valid Svelte. Compiles. Runs. Now fails CI with a message telling the developer to **"fix the malformed
brace"** — there is no malformed brace.

## This already cost a real file

`Sidebar.svelte` appears in PR #925's diff for exactly this reason. Three `//` comments containing
`don't`, `doesn't`, and `ContextMenu.svelte's` had their apostrophes reworded away to get the file to parse.
The author disclosed this fully in the Work Log rather than burying it — but the fix landed in the wrong
file. **Rewording English prose to satisfy a parser does not scale**, and the next developer who writes
`don't` in a comment gets a CI failure that names the wrong cause.

Worth recording what the same investigation revealed, because it argues the parser bug was real and worth
finding: on `main`, those apostrophes silently truncated `Sidebar.svelte`'s scan at line ~742. Lines
**771–984** — the Network and Trash sections — **had never been scanned by the guard at all**. All 12
newly-visible render sites were triaged and none was a raw filesystem name, so nothing was shipping
unescaped. But the guard had been blind to a fifth of that file, and nobody knew.

## What to do

Teach `findMatchingBrace` (and the tag/quote state machine it shares) about JavaScript comments:

- Inside a mustache expression, `//` runs to end-of-line and `/* */` runs to its terminator; quotes and
  braces inside a comment are inert.
- Watch the interaction with template literals and regex literals — `/` is overloaded, and a naive comment
  scanner will misread a regex or a division. Decide how far to go and **write down the boundary**: a
  correct-enough parser with a stated limit beats a subtly wrong one that looks complete.
- Whatever remains unparseable must still fail **closed** and **loud** — do not trade this false positive
  for a return to quiet `[]`. That is the regression to guard against.
- Once comments are understood, revert `Sidebar.svelte`'s three comment rewordings, since the contortion
  is no longer needed. Confirm the file still scans to 38 entries.

## Acceptance criteria

- [ ] `<div on:click={() => { /* it's fine */ }}>` and the `//` equivalent both scan without error.
- [ ] An apostrophe, a double quote, a backtick, and an unbalanced brace inside a comment are all inert.
- [ ] A genuinely unterminated string or brace **outside** a comment still throws loudly — with a test.
- [ ] The template-literal / regex-literal boundary is stated in the module header, and whatever is not
      handled is listed in "What this still cannot see" rather than left implicit.
- [ ] `Sidebar.svelte`'s three reworded comments are restored to natural English and the file still yields
      the same 38 recorded entries.
- [ ] Breaking the comment handling reds a **distinct** test naming the comment case.

## Notes

Found by the Reviewer on **PR #925 / CPE-1761**, 2026-08-17, during the batched sprint. The review noted the
PR's Work Log explicitly declined to file this — "which is how this class of debt goes missing". Related:
CPE-1761 (fail-closed), CPE-1766 (the mid-text render-position gap), CPE-1757 (the parser rewrite).

## Correction from the independent UAT, 2026-08-19

This ticket's acceptance criterion says to confirm the file still scans to **38** entries. The merged
state scans to **39**, and the guard is internally consistent at that number (it matches
`REGISTRY["Sidebar.svelte"]` exactly), so this is not a defect.

The extra entry comes from CPE-1766's mid-text fix landing in the same PR: it surfaced one more
previously-invisible mid-text mustache in that same file, one of the 26 the fix commit reported
across the tree. The stated number was simply written before that fix existed.

Recorded rather than quietly corrected, because a reviewer checking the AC literally would otherwise
see a mismatch and have no way to tell whether the guard or the ticket was wrong.
