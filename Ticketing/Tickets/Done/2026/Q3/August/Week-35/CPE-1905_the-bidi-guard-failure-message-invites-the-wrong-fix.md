---
id: CPE-1905
title: the bidi guard's failure message invites the wrong fix, and misdescribes a duplicate-count drop as a removal
type: bug
priority: Medium
status: Done
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

## Round 2 — review of PR #1104: CHANGES REQUESTED, two blocking findings

Neither touches the mechanism. **F1 was the round-1 defect wearing new words, in the sentence rewritten
because it was wrong** — worth recording as the lesson, not just the fix.

### F1 (blocking) — the doc's list of render positions was written from memory

Parsed from the live REGISTRY literal: `{ text: 1312, title: 170, 'aria-label': 67, '@html': 6 }`,
total 1555. Round 1's replacement parenthetical listed *"an image's alt text"* — **`alt` has zero
entries, it appears nowhere in REGISTRY** — and omitted **`@html`**, which has 6 and is the one sink
where a filesystem-supplied name is a *markup* surface rather than merely a spoofable label. The
sentence immediately before it says REGISTRY "holds the exact list this prose summarizes", which is
exactly what makes the parenthetical read as an enumeration, so the developer this ticket is about goes
hunting for `alt:` keys that do not exist and is never told the `@html:` keys do.

The lesson is CPE-1932's, one scope in: **round 1 fixed a stale claim by writing a fresh unverified one.**
The old sentence ("the exact file/line list") was stale since CPE-1885; the replacement was wrong on the
day it was written. Prose about a data structure has to be derived from the data structure.

Fixed two ways:

1. The paragraph now names the positions as the guard's own vocabulary, in backticks: `` `text` `` for a
   body text node, `` `title` `` for a tooltip, `` `aria-label` `` for a screen-reader label, `` `@html` ``
   for a raw-markup block, "or whatever other attribute the name lands in" — open-ended, because
   `bidiRenderScan.ts` sets `kind` to *any* attribute name it meets.
2. A new derived test, `CPE-1905: the doc's render-position names are exactly the kinds REGISTRY
   records`, checks it **both directions against the live literal**:
   - **REVERSE** — every kind present in REGISTRY must be named, in backticks, in the paragraph. This is
     the airtight leg and it is the one that reds against round 1's shipped paragraph.
   - **FORWARD** — every backticked token shaped like a render position (`^@?[a-z][a-z0-9-]*$`, so
     CamelCase component names and `REGISTRY` are excluded) must be a kind REGISTRY records. This is what
     catches an invented `alt`.

   **Red-proofed both ways, then reverted.** Adding round 1's `alt` back: *"the doc's 'Not yet covered'
   paragraph names render position(s) REGISTRY does not record: alt (REGISTRY records: @html, aria-label,
   text, title)"*. Deleting the `@html` clause: *"REGISTRY records render position(s) the doc's … paragraph
   never names: @html"*.

   **Stated blind spot, at the site, because it is the one that let `alt` in:** the forward leg only sees
   positions written in **backticks**, and round 1's "an image's alt text" was bare prose — the forward
   leg would have walked past it. The reverse leg is the one that would have caught round 1. The paragraph
   is therefore written with every position in backticks so the forward leg has something to bite on, and
   the comment says not to tidy them back into prose. The forward leg is also deliberately over-broad
   (any lowercase backticked token in that paragraph is read as a position claim), which fails toward
   reporting too much.

The guard message's own version of this list (`WHY_THIS_GUARD_EXISTS`) was checked and is **not** wrong —
it explains the key *format* and does include `@html`. Only the doc needed changing.

### F2 (blocking) — the deliverable had no automated coverage of its own

`describeDrift` was module-private and reachable only through a *failing* assertion, so round 1's four
clause shapes could only be observed by the manual sabotage the Reviewer and I each ran by hand. A
refactor that swapped the `f > r` / `f < r` branches, or folded FEWER back into GONE's wording, would
have shipped **green**, re-introducing this exact defect with 5,477 tests passing. That is the repo's own
red-proof rule one level in: round 1 red-proofed the ratchet it *names* and left the thing it *built*
resting on a sabotage nobody can re-run in CI.

`describeDrift` is now exported, with a synthetic-multiset table block (`CPE-1905: the failure message's
four drift clauses`, 8 tests) pinning: one clause per case with the right leading verb for `["a"]/[]`,
`["a","a"]/["a"]`, `["a"]/["a","a"]`, `[]/["a"]`; agreement produces no clause; **every clause ends in
`.`** (finding 3's property, previously unguarded by anything); both counts always present; singular
`1 time` vs plural `2 times`; FEWER and GONE distinct, with `no longer rendered raw` appearing **only**
in GONE; and the position-kind swap yielding two separate sentences, NEW first. Synthetic key multisets,
not component sources — the verdict is still the multiset equality.

**Sabotaged three ways, numbers written at the site, each reverted:**

| sabotage | result |
|---|---|
| swap the `f > r` / `f < r` branches | 4 failed, 21 passed |
| drop the trailing `.` from the NEW clause | 2 failed, both quoting the clause text |
| re-word FEWER back to round 1's `STALE recorded entry(ies), no longer rendered raw` | 3 failed, 22 passed |

In both full-file runs every failure was inside the new block and nothing else moved — no other test in
the file reads a clause at all.

### F3 (non-blocking) — a legitimate raise produces two reds, and the message named one

Re-ran the +1 sabotage against the wider suite: `src/lib/ratchetsDoc.test.ts` fails independently with
*"docs/design/RATCHETS.md's enumeration table disagrees with scripts/ratchet-baselines.mjs"* (1 failed,
12 passed), because that file's enumeration table carries an asserted `today` cell per baseline. A
developer who followed round 1's message exactly — add the licence row — would push and hit a second
failure that reads as unrelated. Both edits are in the same file, so the guidance now says to add the row
**and** update that baseline's `today` cell. The same omission was in `ratchet-baselines.mjs`'s own
`went UP` error text, so it is closed there too — every ratchet in the repo benefits, not just this one.

### Recorded, no change (reviewer)

`WHY_THIS_GUARD_EXISTS` is ~1,900 characters and precedes the drift, in tension with the F5 note about
the useful delta going first. Checked and judged not to hurt: `WHAT DRIFTED:` is a reliable jump target,
the preamble is fixed-size rather than proportional to the failure, and vitest's diff block prints only
the per-file mismatch strings. Noted so the next round does not rediscover it as a defect.

### Round 2 verification

- `npx vitest run` — 363 files, **5,486** tests pass, 62 skipped (round 1: 5,477 — +9 from the two new
  test blocks).
- `npm run check` — 0 errors, 0 warnings.
- `node scripts/ratchet-baselines.mjs compare origin/main` — exit 0, no baseline raised.
- `src/lib/sprintStallControls.test.ts` green, so the edit to `scripts/ratchet-baselines.mjs` kept it LF.

## Closing record — merged as PR #1104 (`4072bebd`), 2026-08-28

**The mechanism was never the problem and was never touched.** Verified byte-identical **three times** by
an independent Reviewer — `siteKey` / `siteKeyMultiset` / `multisetDiff` at md5 `fee81d95…` and `REGISTRY`
at md5 `e34be63c…`, matching the merge base on both review rounds. Every defect here was in **what the
guard says when it fires.**

### 1. The message offered the wrong fix as an equal option, and never stated the threat

It read *"wrap a genuinely new offender in `displaySafeName`/`displaySafePath`, **or** update `REGISTRY`
here…"* — two remedies, equally weighted, with the threat stated nowhere. **A developer meeting this for
the first time, under time pressure, takes the five-second one and registers an actual vulnerability into
the allowlist.** The guard is then green, correct by its own rules, and protecting nothing at that site.

Now a `WHY_THIS_GUARD_EXISTS` block leads with the threat — a filename carrying bidirectional control
characters can **display as something it is not** — then wrapping as the **default**, then REGISTRY as the
**exception needing a stated reason**.

### The deliberate cost: a ratchet that already existed and that the message never mentioned

The obvious remedy — require a comment beside each new entry — was **rejected with the better argument**:
*"an in-file marker is what a make-it-green diff adds to itself."* A convention you can satisfy in the same
edit is not a cost. (Also measured: REGISTRY is **98 files, 1,555 entries, all single-line arrays** — there
is nowhere to hang a per-entry note without reformatting all 98.)

Instead the message now points at `bidi-render-registry`, which counts REGISTRY's entries and is measured
against the merge base by the `ratchet-guard` job — so **an added entry reds unless the same diff writes a
`docs/design/RATCHETS.md` row naming ticket and reason.**

**Verified in both directions, which is what makes it a cost rather than theatre.** One added entry →
`bidi-render-registry … went UP: 1555 -> 1556`, exit **1**. The Reviewer then **added the licence row
itself** and got exit **0**, `RAISED, and declared in docs/design/RATCHETS.md`. The licence is neither
theatre nor a dead end.

### 2. A duplicate-count drop was reported as a removal, which is false

Some components render the same expression twice. Deleting **one** produced *"STALE recorded expression(s),
**no longer rendered raw**"* — but it was still rendered, once. Working out that the count went **2→1**
rather than **1→0** required hand-diffing two arrays inside ~28 other expressions, **and the two situations
call for opposite fixes** (delete one registry line vs delete both).

`describeDrift` now buckets by (found, recorded) and names **both numbers**: NEW / MORE / **FEWER — "NOT a
removal"** / GONE. The `kind:` prefix does **not** already fix this, and the ticket records why: when both
occurrences share a kind, the prefixed form reads exactly as misleadingly as the bare expression.

**The usability claim was judged, not just asserted**, and upheld on a better basis than "different words":
every clause prints `(found, recorded)` in the same shape, so `1 time / 2 times` vs `2 times / 0 times`
separates FEWER from GONE **numerically even if a reader skims the verb** — and each clause names its own
fix, which really are opposite.

### 3 and 4 — the run-on, and what `kind:` means

A NEW clause and a STALE clause were joined by a bare space, so the **position-kind swap** — the exact case
the re-keying exists to catch — produced a run-on at the moment it mattered most. Every clause now ends in
`.`. And nothing had said the `kind:` prefix is a **render position** rather than a type or a filename;
it is now named in the why-block.

### The review found two blocking defects, and the first was the same defect wearing new words

**F1.** The replacement docs sentence was **factually wrong about REGISTRY — in the sentence rewritten
because it was factually wrong about REGISTRY.** Parsed from the live literal:

```
{ text: 1312, title: 170, 'aria-label': 67, '@html': 6 }
```

`alt` has **zero** entries and was listed as one of four positions; **`@html` was omitted** — 6 real
entries, and **the single highest-consequence position in the set**, the one sink where a
filesystem-supplied name is not merely spoofable but a **markup surface**. And the preceding sentence says
*"REGISTRY holds the exact list this prose summarizes"*, which is what makes the parenthetical read as an
enumeration.

Fixed **and derived**, as required. A new doc-parity test checks the paragraph against the live literal
**both ways** — REVERSE (every REGISTRY kind must be named; deleting `@html` reds — *a literal red-proof
against round 1's shipped paragraph*) and FORWARD (every backticked position must be a real kind; adding
`alt` back reds).

**The Reviewer then ran the attack that would have made it read as coverage without being it:** it put
`@html` back **outside** the paragraph, as a new bullet immediately after — **the reverse leg still reds.**
Genuinely paragraph-scoped, so a mention elsewhere cannot silence it.

**And the stated blind spot is real and correctly weighted:** the forward leg matches only **backticked**
tokens, so round 1's bare-prose *"an image's alt text"* would have walked straight past it. The reverse leg
carries the weight, and the comment says not to tidy the backticks away.

**F2 — the deliverable had no coverage of its own.** `describeDrift` was module-private and reachable only
through a *failing* assertion, so its behaviour rested on a manual sabotage nobody can re-run in CI. **A
future refactor swapping the `f > r` / `f < r` branches would ship green with 5,477 tests passing**,
re-introducing the exact defect this ticket removes. *The PR red-proofed the ratchet it names and left the
thing it built unguarded.*

Now exported with an 8-test table over synthetic multisets, sabotaged three ways with the numbers at the
site — **branch swap 4 failed / 21**, **missing period 2 failed**, **FEWER re-worded to round 1's text 3
failed / 22** — all failures inside the new block, all reproduced by the Reviewer, who added a **fourth**
sabotage of its own on the singular/plural helper (**2 failed / 23**) because none of the three exercised it.

### F3 — closed at the shared layer, so every ratchet benefits

The guidance named **one** obligation for a legitimate raise; there are **two reds** — the licence row
*and* the `today` cell in the same doc. Since the omission was also in `ratchet-baselines.mjs`'s own
`went UP` text, **it was fixed there too.**

**Verified end-to-end rather than by reading the wording:** the Reviewer triggered the failure, did
**exactly and only what the message says**, and reached `RATCHET_EXIT=0` with `ratchetsDoc` +
`ratchetBaselines` **80/80 green**. In round 1, following the message did not get you there.

Also checked: the generic advice is correct for **every** gated baseline — the one baseline where it would
have been wrong (`unenforced: true`, a non-integer `today` cell) `continue`s before the error path and never
receives it.

### Recorded, no change requested

The why-block is ~1,900 characters and now precedes the drift, in tension with a standing note that *"the
useful delta goes FIRST"*. The Reviewer checked whether it hurts and concluded it does not — `WHAT DRIFTED:`
is a reliable jump target, the preamble is fixed-size rather than proportional to the failure, and vitest's
diff block prints only the per-file mismatch strings. **Noted so a future round does not rediscover it as a
defect.**

One non-blocking nit left open: the docs' *"or whatever other attribute the name lands in"* is accurate
about how the **key** is formed but slightly generous about **coverage** — the scanner examines only
`title`, `aria-label` and `alt`, and a raw name in `placeholder=` or `data-*` is a documented, deliberate
non-detection.

### Gates at merge

Full suite **363 files / 5,486 passed / 62 skipped** (+9 from the new tests) · `npm run check` **0 errors,
0 warnings** · `ratchet-baselines compare origin/main` 13 enumerated, **no baseline raised** ·
`bidiEscape.guard.test.ts` alone **25/25** · CI `completed success — total_count=26 pending=0 skipped=1
coverage=ok`.

Line endings verified directly rather than argued: the `.mjs` edit is **4 added / 1 deleted**, all three
changed blobs pure LF, no BOM. *(The author cited a test as evidence for this; the Reviewer noted that test
says nothing about the file — right conclusion, wrong evidence, and both were said.)*

**Family:** CPE-1885 (the re-keying whose mechanism this leaves untouched), CPE-1712 (`bidi-filename-spoof`,
the underlying threat), CPE-1757, CPE-1771, CPE-1934 (ratchets), CPE-1948 (the RATCHETS table asserted
against the live measurer — the second red this now names), CPE-1933 (derive provenance; do not name a
backstop without checking it can fire).
