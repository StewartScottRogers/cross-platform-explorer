---
id: CPE-1776
title: The render guard's lookback truncates at 200 characters, silently dropping a render after long alt text
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-17
closed:
---

## Problem

Found by the final gate review on PR #925 (CPE-1761), probing for shapes nobody had asked about. Both
defects are **pre-existing** in `isRenderPosition` and were not introduced by that PR — but they sit in the
exact function it extended, and they are the same silent-drop failure mode the whole CPE-1761 effort exists
to close.

### 1. The 200-character lookback truncates — the solid one

`isRenderPosition` re-derives its context from a fixed-size slice:

```ts
const before = markup.slice(Math.max(0, idx - 200), idx);
```

and then matches `/\b(?:title|aria-label|alt)="[^"]*$/` (and now the single-quote form) against it. If the
attribute value's text between the `title=` / `aria-label=` / `alt=` token and the mustache runs longer than
about 194 characters, **the token itself falls outside the slice**, both regexes fail, and the mustache is
silently classed as a non-render and dropped. Measured:

| Shape | Result |
|---|---|
| `alt="…<194 chars of text…> {entry.name}"` | caught |
| `alt="…>194 chars of text…> {entry.name}"` | **QUIETLY CLEAN — `[]`, no throw** |

Both quote styles are affected identically.

This is not an exotic input. **Verbose `alt` and `aria-label` text is exactly what good accessibility
practice produces** — a long, descriptive label followed by a filename. The guard is silently blind to the
renders most likely to appear in well-written accessible markup.

### 2. Escaped quotes are read inconsistently — the narrower one

The main state machine treats a backslash as an escape (`if (ch === "\\") { i += 2; continue; }`, keeping
`quoteChar` open). `isRenderPosition` re-derives context independently with `[^"]*$` / `[^']*$`, which has
**no concept of backslash-escaping** — so once a literal quote character appears in the lookback, the regex
stops matching and the mustache is dropped.

```
title="she said \"hi\" {entry.name}"   -> [] (quietly clean)
title="she said hi {entry.name}"       -> caught
```

Caveat recorded honestly by the reviewer: it was **not** confirmed that Svelte's compiler accepts a
`\"`-escaped double quote inside a markup attribute value as written. **Establish that first.** If Svelte
rejects that syntax the defect is unreachable and this half should be closed as such, with the finding
recorded — not fixed speculatively.

## The real problem behind both

`isRenderPosition` **re-derives** parser state from a raw text lookback, while the scanner that calls it
already **has** that state — it knows the current tag, the current attribute, and whether it is inside a
quote and how that quote was opened. Two mechanisms deciding the same question will keep disagreeing, and
every disagreement is a silent drop. Both defects above are instances of that one design flaw.

Fix the design, not the two symptoms: pass the state machine's own context into the render-position
decision, or record the attribute name when the scanner encounters it. Widening the slice to 500 characters
would close today's measurement and leave the class open.

## Acceptance criteria

- [ ] A mustache following more than 200 characters of `alt` / `title` / `aria-label` text is detected.
      Test with 200, 500, and 2000 characters of preceding text so the fix is not another fixed window.
- [ ] The escaped-quote question is settled first: either it is unreachable in valid Svelte (recorded, with
      what you checked against), or it is detected.
- [ ] `isRenderPosition` no longer re-derives context the caller already holds — or, if it must, the
      duplication is documented with why the two cannot disagree.
- [ ] Breaking each fix reds a **distinct** test naming the shape, not a generic set mismatch.
- [ ] The 136-file sweep still throws 0, and the REGISTRY delta is reported. **If new offenders surface,
      triage each individually — do not bulk-record them.** New offenders would mean real unescaped renders
      have been shipping behind this gap, which is a finding, not a chore.
- [ ] The module header's "What this still cannot see" list is accurate afterwards. It currently omits both
      of these, so a reader treating that list as the complete boundary is miscalibrated.

## Notes

Found by the final gate review on **PR #925 / CPE-1761**, 2026-08-17, during the batched sprint. #925 was
merged as a strict improvement rather than held for these, since they are pre-existing and it closed three
fail-open holes. Related: CPE-1761, CPE-1766 (mid-text mustache), CPE-1767 (apostrophe in a comment),
CPE-1768 (41 of 136 components covered), CPE-1757.
