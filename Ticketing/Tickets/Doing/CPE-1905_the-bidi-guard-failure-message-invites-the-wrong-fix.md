---
id: CPE-1905
title: the bidi guard's failure message invites the wrong fix, and misdescribes a duplicate-count drop as a removal
type: bug
priority: Medium
status: Doing
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

CPE-1885 fixed the bidi-escape guard's registry keying and its mechanism is sound — reformatting no
longer breaks it, new offenders and stale entries both red correctly, and the multiset genuinely
catches a duplicate count changing. Two problems are left in what it *says* when it fires.

**1. The message offers the wrong fix as an equal option, and never says why it matters.**

On a new raw render the guidance reads: *"wrap a genuinely new offender in
`displaySafeName`/`displaySafePath`, **or** update `REGISTRY` here…"* — two remedies, equally weighted,
and nothing anywhere explains the threat.

The threat is real and specific: a filename carrying bidirectional-text control characters can *display*
as something other than what it is. That is the entire reason this guard exists. A developer who has
never met it, under time pressure, reading two options where one turns the test green in five seconds,
will take the five-second one — **registering an actual vulnerability into the allowlist** and calling
it done. The guard would then be green, correct by its own rules, and protecting nothing at that site.

CPE-1885's own ticket text raised this as a "Consider" rather than an acceptance criterion, so its UAT
correctly passed the PR while flagging it. It is worth closing properly.

**2. A duplicate-count drop is reported as a removal, which is false.**

Some components render the same expression twice (TrashView has two `$t("trash.moreActions")`). Delete
**one** of the two and the guard reds with:

    TrashView.svelte: STALE recorded expression(s), no longer rendered raw: $t("trash.moreActions")

The expression is still rendered — once, at line 360. The message says it is *"no longer rendered
raw"*, which is simply not what happened. To work out that the count went 2→1 rather than 1→0, a
developer has to hand-diff the `found` array against the `recorded` array inside a wall of roughly 28
other expressions.

The mechanism is right; the wording actively misleads about which situation you are in — and the two
situations call for opposite fixes (delete one registry line, versus delete both).

## Acceptance criteria

- [ ] Lead the new-offender message with the **why**, in one sentence, before either remedy: a raw
      render lets a bidi-spoofed filename display as something it is not. Then present wrapping as the
      default and registry-update as the exception that needs a stated reason.
- [ ] Make registering a new offender cost something deliberate — require a comment, a ticket
      reference, or a dated note beside the entry, so "make it green" is not the path of least
      resistance. Decide the shape and record why.
- [ ] Distinguish a count change from a removal in the message itself: say that the expression is still
      rendered N times but recorded M times, and name both numbers. Do not make the reader diff two
      arrays.
- [ ] Red-proof all three message paths and paste the new text: new offender, full removal, and
      duplicate-count drop. The test for this is whether someone reading only the message can tell the
      three apart — say so in the Work Log rather than only asserting on substrings.
- [ ] Do not weaken the mechanism while improving the prose. CPE-1885's multiset comparison and its
      expression-text keying stay exactly as they are; re-run its acceptance cases afterwards.

## Notes

Filed 2026-08-26 from CPE-1885's independent UAT, which passed the PR (all five acceptance criteria
genuinely met, verified case by case) and flagged both of these as usability sharp edges rather than
correctness failures. That framing is right — this is not a defect in CPE-1885's work, it is the next
increment.

Related: **CPE-1885** (the re-keying), **CPE-1712** (`cpe-1712-bidi-filename-spoof`, the underlying
threat), **CPE-1757** (`cpe-1757-bidi-guard-test`), **CPE-1771** (manifest mojibake guard).

Worth pairing with a look at `src/docs/03-explorer.md`'s "Not yet covered" list, which the current
message already points at — if a developer is being told to disclose a gap there, the docs side of that
flow should be as clear as the test side.

## Third defect, found against attempt 2's new message format (2026-08-26)

CPE-1885's attempt 2 re-keyed the registry to `(kind, expression)`, so entries now print as
`text:baseName(path)` and `title:outDir`. A focused re-UAT of the new wording found both original
findings still stand, and one new one.

**A NEW clause and a STALE clause are joined by a bare space, with no punctuation.** The
position-kind swap — the exact case attempt 2 exists to catch — produces:

    SplitFileDialog.svelte: NEW raw offender(s) (kind:expr): title:baseName(path) STALE recorded entry(ies), no longer rendered raw (kind:expr): text:baseName(path) — full found ...

At a skim, `title:baseName(path)` and `STALE` run together with nothing marking the sentence
boundary. Parsed correctly it is exactly the right information — *this expression was fixed at one
sink and reappeared at a different one*, which tells the developer they are looking at a **moved
risk**, not a confused guard. The run-on formatting works against that reading at precisely the moment
it matters most: two clauses, one failure, one file.

Fix: insert a period or `; ` between the NEW and STALE clauses in the `mismatches.push(...)`
construction in `bidiEscape.guard.test.ts`. Purely a string template; it does not touch the mechanism,
which an independent reviewer byte-verified sound.

## Why the `kind:` prefix does not fix finding #2

Worth recording so nobody assumes attempt 2 already closed it. The re-UAT reproduced finding #2
verbatim on `SplitFileDialog.svelte` and explained precisely why the new format does not help:

Both the surviving occurrence and the deleted one share the same kind (`text`), so
`text:baseName(path)` going STALE reads exactly as misleadingly as bare `baseName(path)` did. The
prefix only disambiguates duplicates whose **kinds differ** — TrashView's `title:` / `aria-label:`
pair. For same-kind duplicates it adds a token and no signal.

## And on readability of the prefix itself

Judged an improvement, not a solved problem. `title:` / `aria-label:` / `@html:` map onto their HTML
meaning immediately, and `text:` reads as body text by elimination — but only because the vocabulary
happens to overlap with familiar HTML. Nothing in the message states that the prefix is a *render
position* rather than a type or a filename. Worth one clause of explanation wherever the "why" from
finding #1 lands.

## Work Log — 2026-08-28

Everything below is prose and reporting. **The mechanism is untouched**: the verdict is still
`JSON.stringify(siteKeyMultiset(sites)) !== JSON.stringify(recordedSorted)` over the CPE-1885
`(kind, expression)` multiset, `siteKey`/`siteKeyMultiset`/`multisetDiff` are unedited, and REGISTRY's
recorded keys are unchanged. All 16 tests in `bidiEscape.guard.test.ts` — including CPE-1885's own
red-proofs (the position-kind swap on `SplitFileDialog.svelte`, CPE-1761 #2's in-place substitution on
`PreviewPane.svelte:1015`) — pass unmodified, and the whole suite is 363 files / 5,477 tests green.

### Finding 1 — why first, wrapping as the default, recording as the exception

The one-sentence "wrap …, or update REGISTRY here" was replaced by a `WHY_THIS_GUARD_EXISTS` constant
that opens with the threat, then names one default and one exception:

> WHY THIS MATTERS FIRST: a filename can carry invisible bidirectional-text control characters that make
> it DISPLAY as something it is not (an override character plus "gnp.txt" reads as "txt.png") — a raw
> render is a real filename-spoofing surface, which is the only reason this guard exists (CPE-1712).
> DEFAULT FIX: wrap the expression in displaySafeName(…)/displaySafePath(…) from src/lib/filename.ts. Do
> that unless you can state why this value can never carry a filesystem-supplied name or path.
> EXCEPTION, and it needs that stated reason: record the (kind:expr) pair in REGISTRY here. Only correct
> for something provably not a name — a count, a static literal, an $t("…") key — and it is deliberately
> NOT the cheap way out: REGISTRY's entry total is the one-way ratchet "bidi-render-registry", so adding
> an entry reds the ratchet-guard CI job unless THIS SAME DIFF adds a row to docs/design/RATCHETS.md
> naming the baseline, both values, the ticket and the reason. And if it is a real disclosed gap (a
> filesystem name or path that genuinely still renders raw), name the component in
> src/docs/03-explorer.md's "Not yet covered" bullet too — as `ComponentName` in backticks, no .svelte
> suffix — and add it to DISCLOSED_GAPS in src/lib/bidiEscape.guard.test.ts; the doc-parity test there
> checks the two lists against each other in both directions.
> READING THE KEYS: the prefix before the colon is the RENDER POSITION the expression reaches, not a
> type and not a filename — "text:" is a body text node, "title:"/"aria-label:"/"alt:" are that
> attribute's value, "@html:" is an {@html …} block. The same expression at two positions is two separate
> entries, and moving it from one position to another is a moved risk, not a fixed one.
> WHAT DRIFTED: …

That last paragraph is finding #4, closed in the place finding #1's "why" landed.

### The deliberate cost — the shape chosen, and why

**Shape: surface the ratchet that already exists, rather than invent a per-entry convention.**
`bidi-render-registry` (`scripts/ratchet-baselines.mjs`) already counts REGISTRY's total entries, and the
`ratchet-guard` CI job measures it against the merge base. Raising it requires a row in
`docs/design/RATCHETS.md` naming the baseline, both values, the ticket and the reason — which *is* the
"ticket reference / dated note" the ticket asked for, in a place the raising diff cannot supply to
itself, and which a human reviews. It costs nothing to enforce and nothing to maintain; what was missing
was that **the failure message never mentioned it**, so the developer being tempted had no idea a cost
existed. It is now the second half of the EXCEPTION sentence.

Rejected alternative: a required comment/ticket-ref beside each entry. REGISTRY's values are single-line
arrays holding well over a thousand entries across 92 files; there is nowhere to put a per-entry comment
without reformatting all of them, retrofitting the existing entries is a different (large) job, and an
in-file marker is exactly the kind of thing a "make it green" diff adds to itself.

**Backstop sabotaged rather than asserted** (repo rule: do not name a backstop without checking it can
fire). On this branch, `"InspectCryptoDialog.svelte": []` was changed to `["text:entry.name"]` — one
entry, nothing else touched — and `node scripts/ratchet-baselines.mjs compare origin/main` printed
`::error::bidi-render-registry (src/lib/bidiEscape.guard.test.ts) went UP: 1555 -> 1556` and exited
**1**. Reverted. Those numbers are written into the `WHY_THIS_GUARD_EXISTS` doc comment at the site, not
only here.

### Findings 2 and 3 — `describeDrift`

The two raw `multisetDiff` dumps are no longer what gets printed. `describeDrift` buckets each
disagreeing key by **(found count, recorded count)** and always prints both numbers:

| found vs recorded | clause |
|---|---|
| recorded 0 | `NEW raw render, never recorded here …` |
| found > recorded > 0 | `MORE occurrences of an ALREADY-recorded render …` |
| 0 < found < recorded | `FEWER occurrences — NOT a removal: this expression is STILL rendered raw …` |
| found 0 | `GONE — no longer rendered raw at this position at all …` |

Every clause is a complete sentence ending in `.`, which closes finding 3: a NEW clause and a STALE
clause can no longer collide.

### Red-proof — the three (four) message paths, run live

Each was induced by mutating real component source, running
`npx vitest run src/lib/bidiEscape.guard.test.ts -t "EXACTLY"`, capturing the per-file clause, and
reverting. Verbatim output (the shared `WHY THIS MATTERS FIRST …` preamble above precedes all four):

**(a) New offender** — inserted `<span class="src-hint" title={baseName(path)}>source</span>` into
`SplitFileDialog.svelte`:

    SplitFileDialog.svelte: NEW raw render, never recorded here — wrap it, or re-read the guidance above
    before recording it (kind:expr): title:baseName(path) (rendered 1 time). Full detail — found …

**(b) Full removal** — wrapped `TrashView.svelte`'s `{degradedMessage}` in `displaySafeName(…)`:

    TrashView.svelte: GONE — no longer rendered raw at this position at all; delete every recorded entry
    for it (kind:expr): text:degradedMessage (recorded 1 time, now rendered 0 times). Full detail — found …

**(c) Duplicate-count drop** — wrapped ONE of `SplitFileDialog.svelte`'s two body-text `baseName(path)`
renders (the ticket's exact scenario):

    SplitFileDialog.svelte: FEWER occurrences — NOT a removal: this expression is STILL rendered raw at
    this position, just fewer times, so delete only the surplus recorded entries, not all of them
    (kind:expr): text:baseName(path) (STILL rendered 1 time, recorded 2 times). Full detail — found …

Before this change the same mutation printed `STALE recorded entry(ies), no longer rendered raw
(kind:expr): text:baseName(path)` — false, and pointing at the opposite fix.

**(d) The NEW + STALE run-on (finding 3)** — (a) and (c) together, i.e. the position-kind swap:

    SplitFileDialog.svelte: NEW raw render, never recorded here — wrap it, or re-read the guidance above
    before recording it (kind:expr): title:baseName(path) (rendered 1 time). FEWER occurrences — NOT a
    removal: this expression is STILL rendered raw at this position, just fewer times, so delete only the
    surplus recorded entries, not all of them (kind:expr): text:baseName(path) (STILL rendered 1 time,
    recorded 2 times). Full detail — found …

**Can a reader of ONLY the message tell the three apart?** Asserting on substrings would not answer
that, so here is the judgement, made by reading the four strings above with the code out of view. Yes,
and on three independent cues rather than one:

- **The verb differs and leads the clause.** NEW / MORE / FEWER / GONE are the first word after the
  filename. A skim that reads nothing else already separates "something appeared" from "something
  changed size" from "something left".
- **Both counts are always present, in the same shape.** `(rendered 1 time)`, `(recorded 1 time, now
  rendered 0 times)`, `(STILL rendered 1 time, recorded 2 times)`. Cases (b) and (c) are the pair that
  used to be indistinguishable; they now differ in the only number that matters — `now rendered 0 times`
  versus `STILL rendered 1 time` — without opening either array.
- **Each clause names its own fix, and the two fixes are opposite.** (b) says *delete every recorded
  entry for it*; (c) says *delete only the surplus recorded entries, not all of them*. That is the
  decision the old message got wrong, stated rather than left to be inferred.

Case (d) reads as two sentences about one file — a `title:` render appeared, a `text:` render lost one
occurrence — which is the "moved risk" reading the ticket wanted, and it survives a skim now that the
period is there.

### Docs side

`src/docs/03-explorer.md`'s "Not yet covered" bullet ended with *"see the guard test's `REGISTRY` for the
exact file/line list this prose summarizes"* — stale since CPE-1885 dropped line numbers from the key,
in the one sentence a developer follows to find the list. Rewritten to describe what REGISTRY actually
records now (expression + render position, deliberately not a line number) and to state the doc-parity
check in both directions, matching what the guard message now tells the developer to do. The claim is
scoped to what the test actually checks — this paragraph against the guard's disclosed-gap list — rather
than the broader "any new raw render fails CI", which is a different test in the same file.

### Verification

- `npx vitest run src/lib/bidiEscape.guard.test.ts` — 16/16 pass.
- `npx vitest run` — 363 files, 5,477 tests pass, 62 skipped.
- `npm run check` — 0 errors, 0 warnings.
- `node scripts/ratchet-baselines.mjs compare origin/main` — no baseline raised (all 13 unchanged), so
  `docs/design/RATCHETS.md` needs no new row.
