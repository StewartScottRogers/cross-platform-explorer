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

- [x] An unmatched `{` (and an unmatched `<`) causes the guard to **fail loudly**, naming the file and the
      position — never to report an empty offender set.
- [x] A test proves it: a file containing a lone `{` above a raw render must red, and the failure message
      must say the scan could not be completed rather than that the file is clean.
- [x] Substituting a raw filesystem name into an already-recorded line reds the guard. Demonstrate on
      `PreviewPane.svelte:1015` specifically, since that is the measured instance.
- [x] The recorded-set comparison stays readable — a developer seeing the failure must be able to tell which
      expression changed, not just that a hash differs.
- [x] The two stale doc phrases are corrected, and consider whether the parity test can cover the wording it
      points at.
- [x] The guard's header list of what it "still cannot see" is updated to match what remains true after this
      ticket.

## Notes

Related: CPE-1757 (PR #918 — the guard), CPE-1712 (the spoof fix it protects), CPE-1760 (the prop-pass-through
leaf, blind spot #4 in the same header).

## Work Log

Branch `CPE-1761-render-guard-fails-closed`.

**#1 — unmatched `{` fails loudly.** `handleMustache` (`src/lib/bidiRenderScan.ts`) used to set
`i = markup.length` whenever `findMatchingBrace` returned `-1`, silently ending the scan of the whole
rest of the file. It now throws a new `RenderScanError` (file label + 1-based line/col + a message that
explicitly says "this does NOT mean the file is clean"). Same fail-open shape existed for an
unterminated `<!--` comment and an unterminated `</...>` closing tag (both fall back to
`i = markup.length` too) — fixed identically, since they're the same defect class even though the
ticket named only `{`/`<`.

**#3 — stray `<` fails loudly instead of silently misclassifying.** In the body-text branch of
`findUnsafeRenderLines`'s state machine, ANY `<` used to flip the scanner into `inTag` mode, including a
bare comparison like `a < b` in prose. That silently reclassified every mustache up to the next real `>`
as an ATTRIBUTE-position mustache (only counted for `title=`/`aria-label=`/`alt=`), suppressing a genuine
body-text render. Fixed: only a `<` immediately followed by a letter, `/`, or as part of `<!--` opens tag
mode; anything else throws the same `RenderScanError`.

**#2 — line:expr, not line.** `findUnsafeRenderLines` now returns sorted `"${line}:${expr}"` strings
(`expr` whitespace-normalized to one line for readability) instead of bare line numbers, via a new
exported `compareOffenders` comparator (numeric-by-line, then lexical) used both inside the module and by
the guard test's REGISTRY sort. `REGISTRY`'s ~700 entries across 41 components + `APP_MARKUP_OFFENDERS`
were regenerated **mechanically** (a throwaway script ran the fixed `findUnsafeRenderLines` over every
registered file's real, unmodified source and captured its exact output — no hand-typing) and pasted in
verbatim. Verified green both before (the generation run itself succeeded — see the Sidebar.svelte finding
below for the one exception) and after (full `bidiEscape.guard.test.ts` run, 7/7 green; full suite below).
A side effect of moving from a `Set<number>` to `Set<string>` keyed by `line:expr`: two different unsafe
expressions on the same line (e.g. `PreviewPane.svelte`'s wrapped-render regression test) no longer
collapse into one recorded line — `bidiRenderScan.test.ts`'s own "reports the correct line for a real
spoof case" test now asserts both `3:g.name` and `3:g.path` where it used to assert a single `3`.

**Real defect surfaced by the fail-loud fix (decide-and-log).** Regenerating the registry threw
`RenderScanError` on `Sidebar.svelte` alone: `on:contextmenu`'s handler (line 757) has a `//` comment
containing an apostrophe ("...place rows don't (unchanged)."). `findMatchingBrace`'s naive
string-literal tracker has no concept of a `//` line comment, so it read that apostrophe as OPENING a
single-quoted string, then a second apostrophe two lines later ("doesn't") closed it, then a THIRD
apostrophe ("ContextMenu.svelte's") opened a new fake string that was never closed before EOF — hence
"unterminated `{`" at the outer `on:contextmenu={` brace. This is a real, pre-existing engine limitation
(comments inside inline tag-attribute JS aren't stripped the way `<script>` blocks are), previously
INVISIBLE because the old code silently truncated the scan right there instead of erroring — proof the
fail-loud fix earns its keep on a real file, not just the synthetic repros. Fixed narrowly: reworded the
three affected comment lines to avoid apostrophes (no logic change), rather than teaching the scanner
about `//` comments generally — that's a materially bigger, separately-scoped change (proper JS
tokenization) not asked for by this ticket. Logging it here as a candidate follow-up rather than filing a
new ticket unprompted. Fixing it also retroactively revealed ~12 previously-NEVER-SCANNED offender lines
in `Sidebar.svelte`'s Network/Trash sections (lines 771–984, entirely past the old truncation point) — all
harmless UI text (`Collapse`/`Expand` toggles, `$t(...)` i18n, `formatSize`, one `displaySafePath` call
whose surrounding ternary trips the documented false-positive), consistent with the rest of REGISTRY, so
recorded as-is per the ticket's "migrate mechanically" instruction — no wrapping needed.

**Doc fixes.** `src/docs/03-explorer.md:83` "grep-based guard test" → "parser-based guard test" (it has
been a parser since CPE-1757 round 2). `:92` "the guard test's `ALLOWLIST`" → "the guard test's
`REGISTRY`" (renamed in round 2; `ALLOWLIST` no longer exists as an export). Added a new guard test,
`the doc does not use the stale round-1 vocabulary (ALLOWLIST / grep-based) for this guard`, so this
specific drift is now covered by the parity test per the AC's "consider whether the parity test can cover
the wording it points at" — previously only the DISCLOSED_GAPS names were checked, not this prose.

**Header updated (AC bullet 6).** `bidiRenderScan.ts`'s module docstring gained a paragraph describing
CPE-1761's two changes (line:expr pinning, fail-loud on unparseable markup) and a new "still cannot see"
bullet documenting the trade: a genuinely literal `<` in body text now hard-errors the whole scan rather
than being silently tolerated (a deliberate loud-failure-over-quiet-fail-open trade, but a real limitation
worth flagging for future readers). Also fixed the same `ALLOWLIST`→`REGISTRY` wording drift inside this
file's own header (line ~15), since I was already touching it.

**UAT follow-up — criterion 6 was initially incomplete.** UAT verified all three fail-closed fixes hold on
main and red loudly on this branch, but flagged that the "still cannot see" header omitted a real,
pre-existing (not a CPE-1761 regression) blind spot: `isRenderPosition` only treats a mustache as a render
position when the text immediately before `{` ends in `>` or `}`, so ordinary prose between a tag boundary
and the mustache defeats it — `<div>File: {entry.name} was found</div>` scans clean on both main and this
branch. Per the Foreman's explicit direction, did NOT touch `isRenderPosition` here (fixing it surfaces new
offenders across all 41 registry files, each needing individual triage — that's CPE-1766's whole scope, and
folding it into this PR would bury the three fail-closed fixes in a much larger, riskier diff). Added the
missing header bullet instead, referencing CPE-1766 (filed by the Foreman with the full measurement and the
required 41-file rescan) as the ticket that owns the actual gap.

**New tests (each proven to bite — broke the fix, ran the test, confirmed red, then restored):**
- `bidiRenderScan.test.ts` → describe "CPE-1761: the scan fails loudly instead of silently truncating":
  - `control: baseline-two-raw` — sanity: 2 real offenders normally caught (`["2:entry.name","3:other.path"]`).
  - `#1 brace-in-text` — ticket's literal repro (`<p>use { as a brace</p>` above 2 real offenders) now
    throws `RenderScanError` naming the file and matching `/unterminated "\{"/` and
    `/does NOT mean the file is clean/`. RED PROOF: reverted the `close === -1` check to the old
    `i = markup.length` fallback → `AssertionError: an unmatched '{' must throw ... expected undefined to
    be an instance of RenderScanError`. Restored, reran green.
  - `#1 unterminated-mustache` — ticket's literal repro (`<div title="{">`). RED PROOF: same revert →
    `AssertionError: expected function to throw an error, but it didn't`. Restored, reran green.
  - `#3 stray '<'` — `<div>< {entry.name}</div>` (control: without the stray `<`, this construction is
    already covered elsewhere and flags line 1) now throws, message matches
    `/"<" is not followed by a tag name/`; plus the same above-two-real-offenders shape as #1. RED PROOF:
    reverted the letter/`/`/`!--` gate back to unconditional `inTag = true` → `AssertionError: a stray '<'
    ... expected undefined to be an instance of RenderScanError`. Restored, reran green.
  - `a real, well-formed comparison/closing-tag/comment is NOT affected` — guards against an over-eager
    fix: `<!-- a < b, not a tag -->` and a normal closing `</div>` still work and still find the real
    offender (not broken by the new checks).
- `bidiEscape.guard.test.ts`:
  - `CPE-1761 #2: substituting a raw filesystem name into an already-recorded line reds the guard
    (PreviewPane.svelte:1015)` — takes the REAL file, asserts the fixture line still matches, replaces
    `title={$t(action.labelKey)}` with `title={entry.name}` in place, and asserts (a) line 1015 is STILL
    an offender either way (proving a line-number-only check would have stayed green — the actual bug),
    (b) the new entry is `1015:entry.name` and the old `1015:$t(action.labelKey)` is gone (the REASON),
    and (c) the resulting set no longer equals REGISTRY's recorded array (what actually reds the real
    guard test). RED PROOF: reverted `offenders.add` to record `${line}` only (no expr) →
    `AssertionError: line 1015 should still be an offender after the substitution: expected false to be
    true` — i.e. the naive re-check itself broke first, which is exactly the right failure (the old code
    couldn't even represent "same line, different expr" as still-present). Restored, reran green.
  - `the doc does not use the stale round-1 vocabulary (ALLOWLIST / grep-based) for this guard` — asserts
    absence of both stale phrases plus presence of `REGISTRY`, closing the AC's "consider whether the
    parity test can cover the wording" ask.

**Verification.** `npm run check` — 0 errors, 0 warnings. Full `npx vitest run` — 312 files / 4063 tests,
all green (includes the pre-existing `bidiRenderScan.test.ts` 27→32 tests and `bidiEscape.guard.test.ts`
5→7 tests, all passing). `git diff --numstat` confirmed no whole-file rewrites / encoding corruption.

**Assumptions (decide-and-log, no questions asked):**
- Extended the "fail loudly instead of silently truncating" treatment to unterminated `<!--...-->` and
  unterminated `</...>` as well as the ticket's named `{`/`<` cases, since they're the identical
  `i = markup.length` fail-open pattern and leaving them would mean "never report an empty offender set"
  wasn't actually guaranteed.
- Fixed the Sidebar.svelte apostrophe-in-comment defect narrowly (reworded 3 comment lines) rather than
  teaching the brace-scanner about `//` comments in general — the latter is a real, separately-scoped
  improvement (JS tokenization inside inline tag-attribute expressions) that this ticket didn't ask for
  and that would meaningfully grow the diff's risk surface.
- Newly-surfaced Sidebar.svelte offender lines (771–984) were recorded as-is, not wrapped/fixed — none are
  genuine raw filesystem names (all UI toggle labels, i18n, formatted sizes, or an already-wrapped
  `displaySafePath` call tripping the documented ternary-condition false positive), consistent with the
  rest of REGISTRY's character, and `Sidebar.svelte` is already a `DISCLOSED_GAPS` entry so a fuller true
  picture doesn't contradict any doc claim.
- Did not touch `src-tauri`/Rust or any other backend surface — this ticket is entirely `src/lib` +
  `src/docs`.

## Attempt 2 — independent Reviewer findings, all addressed

**F1 (BLOCKER) — an unbalanced attribute quote still failed open silently.** `quoteChar`/`inTag` were
never checked at EOF, so `findUnsafeRenderLines` could fall off the end of `markup` mid-tag/mid-quote and
return whatever it found so far — an odd number of `'`/`"` in an attribute (e.g. `title='it's a file'`)
silently swallowed every render below it as an (unflagged) attribute-position mustache. Fixed:
`src/lib/bidiRenderScan.ts`, immediately before the final `return`, added
`if (quoteChar !== null) fail(...)` and `if (inTag) fail(...)`. Both throw `RenderScanError` naming
"reached end of file inside an unterminated ' attribute string" / "...unterminated tag — no closing
'>' was found". Ran across all 136 `.svelte` files under `src/` (a throwaway script, same shape as the
registry-generation one) — 0 trip it, so this costs nothing on real files. New tests (bit and proven, see
below): "F1: an unbalanced attribute quote must RED at EOF", "F1: an unterminated tag ... must RED at
EOF", and an end-to-end probe reproducing the reviewer's exact DiskSpaceView.svelte-shaped repro.

**F3 — two existing fail-open fixes had NO test that bit.** Reverting the unterminated-`<!--` hunk or the
unterminated-`</` hunk independently left the suite 39/39 green — untested production code covering two of
the ACs. Added two tests to the `CPE-1761: the scan fails loudly...` describe in
`src/lib/bidiRenderScan.test.ts` (`<!-- unterminated\n{entry.name}` and `</div\n{entry.name}`), each
proven red by reverting its fix (see red-proof section below) before restoring.

**F2 — the header made a false absolute claim.** `bidiRenderScan.ts`'s new paragraph said the guard "can
never silently claim a file is clean when it wasn't sure" — false once F4b/F4c's known heuristic gaps are
counted (the scanner IS confident, just wrong, on those shapes — a different failure class than "unsure").
Reworded to say the guard closes the specific silent-truncation-to-`[]` shape round 2 shipped, and
explicitly disclaims that this is not a blanket exhaustiveness guarantee, pointing at "still cannot see"
for the known heuristic gaps.

**F4a — disclosed the apostrophe-in-comment trade + filed for CPE-1767.** Added a "still cannot see"
bullet (right after the CPE-1761 fail-loud bullet) naming that `findMatchingBrace` has no concept of `//`
or `/* */` JS comments, so an apostrophe inside a comment written INSIDE an inline tag-attribute expression
(e.g. `on:click={() => { /* it's fine */ }}`) — valid, harmless Svelte — now hard-errors the scan with a
message that doesn't describe the real cause. Framed explicitly as a hard-error FALSE POSITIVE (safe
direction, real developer cost), referencing CPE-1767 per the Foreman's message.

**F4b — the mid-text gap.** Bullet already added per the Foreman's first message (see "UAT follow-up"
above); unchanged this round.

**F5 — put the delta first.** `bidiEscape.guard.test.ts`'s mismatch message now emits the `NEW`/`STALE`
clauses BEFORE the full `found [...] vs recorded [...]` array dump, so the useful diff isn't buried behind
multi-KB dumps for large files like `AgentTimeline.svelte`.

**F6 — nit.** `bidiRenderScan.ts`'s ternary-condition bullet: "costs an allowlist entry" → "costs a
`REGISTRY` entry" (the same stale-vocabulary drift this ticket fixes in the shipped doc, now fixed in the
module's own header too).

**F7 — nit.** The new `ALLOWLIST`/`grep-based` doc test was checking the WHOLE `03-explorer.md`, which
would red on an unrelated future feature legitimately using either word. Rescoped to the bidi-escape
bullet specifically (same paragraph-extraction technique the sibling "Not yet covered" test uses, anchored
at the bullet's own start rather than its "Not yet covered" sub-heading — anchoring narrower would have
silently stopped covering "grep-based" at all, since that phrase sits earlier in the same bullet, before
the "Not yet covered" sub-string begins).

**Red-proof for the new/changed tests (attempt 2), each broken, run, confirmed red, then restored:**
- `F1: an unbalanced attribute quote must RED at EOF` — reverted both new EOF checks (`if (false &&
  quoteChar !== null)` / `if (false && inTag)`) → all three F1 tests (`unbalanced attribute quote`,
  `unterminated tag`, and the DiskSpaceView-shaped end-to-end probe) failed: `expected undefined to be an
  instance of RenderScanError`, `expected [Function] to throw an error`, `expected function to throw an
  error, but it didn't`. Restored, reran green.
- `an unterminated '<!--' comment ... must RED` — reverted the `<!--` `fail(...)` back to
  `i = end === -1 ? markup.length : end + 3` → `expected undefined to be an instance of RenderScanError`.
  Restored, reran green.
- `an unterminated '</' closing tag ... must RED` — reverted the `</` `fail(...)` back to
  `i = end === -1 ? markup.length : end + 1` → `expected undefined to be an instance of RenderScanError`.
  Restored, reran green.

**Verification (attempt 2).** `npm run check` — 0 errors, 0 warnings. Full `npx vitest run` — 312 files /
4068 tests, all green (bidiRenderScan.test.ts grew 32→37, bidiEscape.guard.test.ts stayed at 7 with F5/F7
edits to existing tests rather than new ones).

## Attempt 3 — the remaining gap: single-quoted render positions were silently DROPPED, not merely EOF-missed

**Root cause (reviewer).** Attempt 2's EOF check is correct as far as it goes, but `quoteChar` is a raw
parity toggle with no concept of "this quote belongs to this tag." An unterminated single-quoted attribute
LATER re-balanced by an unrelated apostrophe elsewhere in the file (any contraction — "it's", "can't",
"you're", "don't") never reaches EOF in a bad state, so attempt 2's check never fires. The actual bug isn't
in the EOF check at all: while `quoteChar === "'"`, a `{` still calls `handleMustache`, and
`isRenderPosition`'s inTag branch only recognized the DOUBLE-quote form (`attr={` / `attr="..{`) — a
single-quoted context (`attr='..{`) matched neither regex, so the mustache was silently classified as a
non-render and DROPPED. Not deferred, not flagged — dropped, regardless of whether the quote ever balances.
Reproduced exactly as the reviewer measured:
```
<button title='Loading>Spinner</button>
<span>{entry.name}</span>
<p>You're all caught up</p>
```
returned `[]` before this fix — a raw `{entry.name}` shipped green.

**Fix.** `src/lib/bidiRenderScan.ts`, `isRenderPosition`'s inTag branch: added
`if (/\b(?:title|aria-label|alt)='[^']*$/.test(before)) return true;` alongside the existing double-quote
regex, making the two quote styles symmetric — `title='{entry.name}'` is now classified exactly like
`title="{entry.name}"`, since both are equally legal Svelte. Chosen over failing closed on single quotes
(which would red on legitimate single-quoted attributes, trading a silent miss for a false positive that
blocks real work) per the reviewer's explicit direction — symmetry removes the drop entirely rather than
relocating it.

**New tests (each proven to bite — reverted the single-quote branch, ran, confirmed red, then restored):**
- `F1 attempt 3: a forgotten closing quote re-balanced by a later contraction must not silently drop the
  render in between` — the reviewer's exact realistic repro. RED PROOF (branch reverted to
  `if (false && ...)`): `AssertionError: expected entry.name to be reported as an offender, got: []:
  expected false to be true`. Restored, reran green.
- `F1 attempt 3: a legitimate single-quoted mustache is a render position, same as the double-quoted form`
  — asserts `title='{entry.name}'` and `title="{entry.name}"` produce IDENTICAL results (not just "both
  non-empty"), so the two forms cannot silently drift apart again. RED PROOF: `AssertionError: expected []
  to deeply equal [ '1:entry.name' ]`. Restored, reran green.
- `F1 attempt 3: the narrower repro` — `<div title='unterminated><span>{entry.name}</span><p>can't see</p>`
  (single-quote analog of the F1-attempt-2 double-quote-shaped probe). RED PROOF: `AssertionError: expected
  entry.name to be reported, got: []: expected false to be true`. Restored, reran green.

**136-file sweep (re-run after the fix).** A throwaway script ran `findUnsafeRenderLines` over every real
`.svelte` file under `src/` (136 files, same walk as attempt 2's sweep): 0 throws, all 136 completed. The
REGISTRY guard test (`bidiEscape.guard.test.ts`, "every registered component's raw-render set matches its
recorded lines EXACTLY") stayed green with ZERO changes required to REGISTRY — the single-quote fix did not
surface any new offender in any of the 41 registered files or App.svelte, so no individual-file triage was
needed (per the Foreman's explicit stop-and-report condition — nothing to report; the set is unchanged).

**Header softened again.** The CPE-1761 module-header paragraph was restructured into three enumerated,
narrowly-scoped points (line:expr pinning / fail-loud-on-unparseable-markup / single-quote symmetry), each
explicitly framed as "closing one way a render could go unreported" rather than a claim of exhaustiveness,
with an explicit non-exhaustiveness disclaimer up front pointing at "still cannot see" for what remains
genuinely invisible (CPE-1766's mid-text gap, CPE-1767's apostrophe-in-comment false positive).

**Verification (attempt 3).** `npm run check` — 0 errors, 0 warnings. Full `npx vitest run` — 312 files /
4071 tests, all green (bidiRenderScan.test.ts grew 37→40).
