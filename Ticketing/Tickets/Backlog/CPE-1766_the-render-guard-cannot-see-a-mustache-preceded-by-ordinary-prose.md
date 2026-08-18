---
id: CPE-1766
title: The render guard cannot see a mustache preceded by ordinary body text, and reports the file clean
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-17
closed:
---

## Problem

Found by the **PR #925 (CPE-1761) UAT**, which was probing for shapes the ticket did not name. It is
**pre-existing on `main`**, identical before and after that PR, and it is **not** one of CPE-1761's three
defects — but it is the same fail-open direction, and it needs no adversarial input at all.

`isRenderPosition` in `src/lib/bidiRenderScan.ts` decides a mustache is a *render position* only when the
characters immediately before the `{` are `>` or `}` (`/[>}]\s*$/`). Ordinary prose between the tag
boundary and the mustache defeats it:

```svelte
<div>hello {entry.name}</div>          ->  []   (quietly clean)
<div>File: {entry.name} was found</div> -> []   (quietly clean)
<div>{entry.name}</div>                 -> caught
```

Measured on both `main` and the CPE-1761 branch — same result, same code path, untouched by that PR.

## Why this is High

The previous three defects needed a stray brace or an in-place edit. **This one needs a sentence.**
`<div>File: {entry.name}</div>` is not a contrived shape; it is how a person writes Svelte. Any raw render
written that way has always been invisible to the guard, on every file, since the guard existed.

That has a second-order consequence worth checking before anything else: the **REGISTRY's ~700 recorded
entries across 41 files may be under-recorded**. Every entry was computed by this same scanner, so any
mid-text render in those files was never recorded — not because it was judged safe, but because it was
never seen. The registry's apparent completeness is evidence of nothing for that shape.

And the failure mode is the one that matters: not a missed catch that reds later, but `[]` — the most
reassuring output the tool can produce.

## What to do

- Fix the render-position test so a mustache in body text is recognised regardless of what precedes it.
  The real question is distinguishing *body text* from *inside an attribute value*, *inside a script/style
  block*, and *inside a Svelte block tag* — not "what character came immediately before".
- **Then re-run the scan across all 41 registry files and diff the result against the recorded set.** Expect
  new offenders. Each one is a pre-existing raw render that has been shipping. Triage them: genuinely unsafe
  ones get escaped; genuinely benign ones get recorded with their expression. Do not bulk-record the
  difference — that would launder real offenders into the registry, which is exactly the failure this whole
  guard family keeps having.
- Update the module header's "What this still cannot see" list to match what is true afterwards. That list
  being stale is what let this sit unnoticed; CPE-1761's UAT flagged it as its criterion-6 miss.

## Acceptance criteria

- [ ] `<div>hello {entry.name}</div>` and `<div>File: {entry.name} was found</div>` are both detected.
- [ ] A mustache inside an attribute value, inside `<script>`/`<style>`, and inside a Svelte block tag are
      still correctly NOT treated as body-text renders — state each with a test.
- [ ] The full 41-file rescan is run and its diff against the recorded set is reported in the PR, with each
      new offender triaged individually (escaped vs recorded-with-reason). No bulk-record.
- [ ] Breaking the fix reds a **distinct** test whose message names the mid-text shape, not a generic
      set-mismatch.
- [ ] The header's "What this still cannot see" list is accurate after the change — and if anything remains
      unseeable, it is listed there explicitly.
- [ ] The guard still never returns a quiet `[]` on markup it could not parse — the CPE-1761 fail-closed
      behaviour is preserved, not regressed.

## Notes

Found by UAT on **PR #925 / CPE-1761**, 2026-08-17, during the batched sprint. Related: CPE-1761 (fail-closed
on unmatched brace/tag, `line:expr` pinning), CPE-1757 (the guard's parser rewrite), CPE-1712, CPE-1760.
