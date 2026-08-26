---
id: CPE-1885
title: key the bidi-escape guard's registry by expression text, not line number — it cost three round-trips in one day
type: task
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-23
closed:
---

## Problem

`src/lib/bidiEscape.guard.test.ts` records each component's raw-render sites as
`"<line>:<expression>"` strings and compares them against a live rescan. Any edit that shifts lines in
a guarded component fails the guard with a wall of "NEW offender" and "STALE recorded" entries that
are **the same expressions at new addresses**.

Three separate round-trips lost to this in a single day (batched run `batched-2026-08-23-1124`):

1. **CPE-1833/CPE-1836** — the worker had to update 13 shifted line numbers for `StatusBar.svelte`.
2. **CPE-1827** — same, for `TrashView.svelte`.
3. **CPE-1827 again** — adding *one line* (`node.focus()` in `clampToAnchor`) shifted every site by 5
   and reddened CI with **27 "new" and 27 "stale" entries**, all of them the identical 27 expressions.
   The Foreman had to apply a mechanical `+5` and re-push.

## Why fix it rather than live with it

The guard is **sound** and must not be weakened. PR #1019's reviewer made the case for it precisely:
it compares against a **live rescan** rather than an allowlist, and requires exact set equality in both
directions, so it cannot silently lie — a new unregistered raw render fails, and a stale entry fails
too. That is the right shape and it has caught real offenders.

The problem is only that it **cries wolf**, and a guard that cries wolf on unrelated edits is a guard
people learn to update reflexively without reading. That is one small step from updating it reflexively
when it has found something real. The cost is not the minutes; it is the erosion.

## What to do

Key each entry by its **matched expression text** (plus the component) instead of its line number. The
expression is what the guard actually cares about — whether a raw, unescaped value reaches the DOM —
and it is stable under reformatting, insertion and deletion.

Watch for the one real wrinkle: a component with the **same expression on two different lines**
(`TrashView.svelte` already has `342:itemCountLabel` and `342:selectedCountLabel` on one line, and
`355`/`356` both `$t("trash.moreActions")`). A bare set of expressions loses that multiplicity, so
count occurrences rather than deduplicating, or key by `expression` plus an occurrence index.

**Prove it still bites.** The whole value is that it fails on a genuinely new raw render:

- add a new unescaped expression to a guarded component → must go **red**
- delete a registered one → must go **red** (the stale-entry direction)
- reformat a guarded component so every line moves, changing nothing else → must stay **green**

That third case is this ticket, and it is the one to demonstrate most carefully.

Consider also whether the failure message can name the *component and expression* first and leave the
addresses out — most of the 27-entry wall above was noise around a one-word fact.

## Acceptance criteria

- [ ] A pure reformat of a guarded component does not fail the guard.
- [ ] A genuinely new raw render still fails it — demonstrated.
- [ ] A deleted registered render still fails it — demonstrated.
- [ ] Duplicate expressions within one component are still counted correctly.
- [ ] The registry no longer contains line numbers, so nothing needs mechanical updating on an edit.

## Notes

Related but distinct: **CPE-1817** fixed the same fragility in a *different* guard, where a call-site
count used a single-line `grep` and mis-fired on a wrapped call. The fix there was to collapse
whitespace before counting. Two guards, one root cause: pinning code by its position in a file rather
than by what it says.

## Work Log

- **2026-08-23 20:40 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`,
  after the third occurrence in one day. PR #1019's reviewer independently recommended the same change
  and correctly noted the guard is self-correcting rather than merely fragile, which is why this is a
  usability fix and not a correctness one.
- **2026-08-26 USMST** — Picked up by a worker; moved Backlog → Doing.
- **2026-08-26 USMST** — Re-keyed `REGISTRY` in `src/lib/bidiEscape.guard.test.ts`: entries are now bare
  expression text (the leading `"<line>:"` stripped from every one of the 92 file entries), compared as
  a MULTISET (occurrence-counted, not deduped) via new `exprMultiset`/`multisetDiff` helpers so a
  component with the same expression on two lines (TrashView's two `$t("trash.moreActions")`) still
  requires two matching entries. `findUnsafeRenderLines`'s own `"<line>:<expr>"` output shape is
  untouched (other tests depend on it), only REGISTRY's recorded keys and the comparison logic changed.
  `APP_MARKUP_OFFENDERS` (App.svelte's separate flat array) is deliberately left as `"<line>:<expr>"` —
  out of scope; the ticket's own examples (StatusBar.svelte, TrashView.svelte) are REGISTRY entries.
  Failure messages now name the component + bare expression first, addresses last. `npm run check`: 0
  errors, 0 warnings. `npx vitest run`: full suite green. Red-proofed for real (reformat stays green;
  delete goes red; new unregistered render goes red) — see the PR description for the exact commands and
  captured output. Opened PR #1029. PR #1026 (sibling, `RevertOutcomePanel.svelte`'s registry entry) was
  still OPEN at push time, not yet merged — rebased onto current `origin/main` before opening the PR.
- **2026-08-26 USMST** — PR #1026 merged to `main` (5f91c852) while PR #1029's CI was running. Rebased
  #1029 onto the new `origin/main` tip; resolved the one conflicting hunk by carrying #1026's
  `RevertOutcomePanel.svelte` addition (the copy-full-list button's label) across into this ticket's
  expression-only format rather than dropping it — confirmed with `git show origin/main:...` before and
  `npx vitest run src/lib/bidiEscape.guard.test.ts` after (15/15 green). Force-pushed the rebase
  (`git push --force-with-lease`); PR settled at SHA `5ef4dc09`, `mergeable: MERGEABLE`. Re-ran `npm run
  check` (0/0) and the full `npx vitest run` (331 files / 4460 tests, +3 over the pre-rebase count from
  #1026's new tests) — both green. Watched CI to a real conclusion on the rebased SHA: all 19 checks
  passed (1 intentionally skipped — GUI smoke windows-latest), confirmed via `gh pr checks 1029
  --json ... | grep pending` returning empty with a stable `total_count` of 19 across repeated reads.
  PR ready for review/merge.
- **2026-08-26 USMST — attempt 2 (CHANGES REQUESTED, blocking finding).** An independent reviewer
  verified round 1 exhaustively (multiset direction-correctness, 0 lost entries across all 98 files
  vs. base `05a36379`, the RevertOutcomePanel.svelte carry-across, `bidiRenderScan.ts` zero-diff, all
  three round-1 red-proofs) and found ONE real hole: keying REGISTRY by bare EXPRESSION TEXT alone is a
  strict information loss versus the old `"<line>:<expr>"` keying whenever the same expression occurs
  more than once in a component — the multiset can count occurrences but cannot tell WHICH is which. A
  future edit that wraps ONE occurrence of a duplicated expression (a real fix) while introducing an
  unrelated brand-new raw occurrence of the IDENTICAL text elsewhere in the same file leaves the total
  count unchanged, so the guard stays green while the unsafe surface silently moves to a new, unreviewed
  line — this ticket's own failure class, one level down, and WORSE in kind: the line-number bug was a
  false POSITIVE (noisy, safe); this is a false NEGATIVE (unsafe content passes). Demonstrated on a real
  file, not a hypothetical: SplitFileDialog.svelte renders `baseName(path)` raw at two real text-node
  positions (101, 114) and `outDir` raw at two real positions (107 title AND text, 168 title).
  **Fix**: keyed REGISTRY by `(expression, render-position KIND)` instead of expression alone. Added
  `UnsafeRenderSite` + `findUnsafeRenderSites` to `bidiRenderScan.ts` (a new function alongside the
  UNCHANGED `findUnsafeRenderLines` — same shared internal scanner, refactored into a private
  `scanUnsafeRenderSites`, zero behavior change to any existing caller; `bidiRenderScan.test.ts` 69/69
  still green). `kind` is `"text"` / `"@html"` / the exact attribute name (`"title"`, `"aria-label"`,
  `"alt"`) — never a line address, so it is exactly as stable under reformatting as bare expression text
  was. REGISTRY's 98 entries were regenerated PROGRAMMATICALLY through `findUnsafeRenderSites` (not
  hand-edited) to guarantee correctness; this surfaced 29 previously-invisible occurrences across 20
  files — all the `title={x}>{x}`-shaped same-line dual-position case `findUnsafeRenderLines`'s `Set`
  has always silently collapsed to one entry (e.g. SplitFileDialog.svelte:107 was one recorded `outDir`
  offender, is now the two real occurrences — `title:outDir` and `text:outDir` — it always was). Guard
  test's helpers renamed `exprMultiset`→`siteKeyMultiset`/`siteKey`, still comparing by MULTISET (not
  deduplicated membership); the 4 substitution-demonstration tests (PreviewPane/ConfirmDialog/StatusBar/
  AgentMenu) updated to compare `findUnsafeRenderSites` output instead of expression-only.
  **Stated residual (explicit, not implied away)**: `(kind, expr)` does NOT distinguish two occurrences
  of the identical expression in the identical kind within one file — SplitFileDialog.svelte's
  `text:baseName(path)` pair stays exactly as ambiguous as it was under the OLD line-keyed design (a
  `Set` entry there could only ever record ONE of a same-line `title={x}>{x}` pair too). Closing that
  needs a full occurrence-index, which reintroduces a position-shaped key and the reformatting fragility
  this ticket exists to remove — documented in both `UnsafeRenderSite`'s doc comment and this file's
  header, not glossed over.
  **Red-proofed for real, live on real files, captured and reverted** (see PR description for exact
  output): (1) the reviewer's exact swap (SplitFileDialog.svelte: wrap one `baseName(path)` text
  occurrence, add a new `title={baseName(path)}` occurrence) — confirmed the OLD expression-only view
  shows NO difference (reconstructed literally in a new permanent test), confirmed the NEW (kind, expr)
  view goes RED naming `NEW raw offender(s): title:baseName(path)` + `STALE ... text:baseName(path)`.
  (2) Reformat immunity re-confirmed: 6 blank lines inserted into StatusBar.svelte, guard stayed GREEN
  (16/16). (3) Duplicate-count both directions re-confirmed on SplitFileDialog.svelte's real
  `baseName(path)` duplicate: wrapping one occurrence (2→1) reds as STALE; adding a third (2→3) reds as
  NEW. `npm run check`: 0/0. `npx vitest run`: 331 files / 4461 tests, all green. Rebasing onto current
  `main` (moved several times since last rebase) before repushing; CI is in a confirmed GitHub Actions
  outage (30-40+ min queued runs org-wide) — not waiting on it, verified locally instead per the
  Foreman's explicit instruction, reporting `CI still pending on <SHA>`.
  Rebase onto `origin/main` (tip `31a2de64`, 4 commits ahead of the last rebase's base — none touching
  `src/lib/bidiEscape.guard.test.ts` or `src/lib/bidiRenderScan.ts`) was conflict-free. Re-ran the guard
  test (16/16), `bidiRenderScan.test.ts` (69/69), `npm run check` (0/0), and the full suite (331 files /
  4461 tests) on the rebased tree — all green. Force-pushed with `--force-with-lease`; PR #1029 settled
  at SHA `09fc2078`. Per the CI-outage warning, did not poll — `mergeable` read back `UNKNOWN`
  immediately after push (GitHub still computing under the outage). **CI still pending on `09fc2078`.**
