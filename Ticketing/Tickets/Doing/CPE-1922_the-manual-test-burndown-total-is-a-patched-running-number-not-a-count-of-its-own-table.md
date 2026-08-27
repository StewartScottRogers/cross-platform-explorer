---
id: CPE-1922
title: MANUAL-TEST-BURNDOWN.md's MVD total is a patched running number, not a count of its own table — it has drifted 2-4 rows in both directions
type: bug
priority: Medium
status: In Progress
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

`.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` carries a running Manual Verification Debt total
in its header. That number is **maintained by patching it forward** (add the rows a shift added,
subtract the rows it automated) rather than by counting the table. It has drifted.

Independently recounted by PR #1042's UAT tester, bare grep over rows marked `⛰ manual`, excluding
the legend line:

| | doc's stated number | literal recount |
|---|---|---|
| pre-PR #1042 | 18 (10 supplementary as of 2026-08-20 + 2 untallied) | **14** (6 primary + 8 supplementary) |
| post-PR #1042 | 16 | **12** (6 + 6) |

The header's "16 total", carried since 2026-08-20, does not match a fresh count of what was in the
table at that point either — so the drift predates any single shift.

## What is NOT wrong

The substantive claims are sound and were verified: the two rows CPE-1822 flipped really are
automated now, and the CI job named as pinning them (`gui-smoke` on `ubuntu-latest`) really is
**blocking** — `continue-on-error` was explicitly removed from it per CPE-1594, while
`windows-latest` remains a `continue-on-error: true` canary. The two "untallied since 2026-08-20"
additions (CPE-1821, CPE-1833/1836) are real rows genuinely added on 2026-08-23 without the total
being updated. PR #1042's *local* arithmetic was done correctly; it reconciled forward from an
already-drifted baseline.

## Why it matters

This ledger is how the crew claims manual testing is disappearing. A number that is approximately
right and drifting is worse than no number, because it reads as measured. The user's standing goal
is to never test anything by hand — the burden of proof is on this file.

## Acceptance criteria

- [ ] **Recount from the table, once, from scratch.** Reset the header total to the literal count.
      Do not patch it forward again from the old value.
- [ ] Make the total **derived rather than asserted**: a script or test that counts the `⛰ manual`
      rows (and the `🔧 in progress` / `🟡 partial` rows separately) and fails CI when the header
      disagrees with its own table. That is the only fix that stops this recurring — this is the
      third bookkeeping correction in this file's history.
- [ ] Decide and document how `🔧 in progress` and `🟡 partial` rows count toward MVD. The ambiguity
      between "6 primary" (which includes them) and "4 primary manual" (which does not) is part of
      how the drift happened.
- [ ] While in there: record the Trash-view surfaces that CPE-1822 did **not** photograph, so they
      are visible debt rather than assumed-covered — row **selection** state ("N selected"), the
      Empty/Restore **ConfirmDialog** (the highest-stakes screen in that view), the
      **restoreErrors** banner, the **overflow menu**, narrow width, and long-filename truncation.
- [ ] Also note the one inconsistency found in the new specs: `trash-degraded-scrolled` is captured
      **dark-only**, despite its test title claiming "in both themes". Either fix the spec or record
      it honestly.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1042's independent UAT recount. This is QA-Architect
work — it is about the measuring instrument, not any one surface.

## Work Log

**2026-08-27 — implemented.** Branched off `main` at `efeac8e9`.

**The true number is 12; the header said 16 — a drift of +4, OVERSTATING the debt.** That independently
reproduces PR #1042's UAT recount (12 = 6 primary + 6 supplementary post-#1042), arrived at by a parser
rather than by grep. Breakdown: primary = rows #10/#11/#12/#14 at `⛰`, row #3 at `🔧`, row #6 at `🟡`;
supplementary = CPE-1577, CPE-1570/1576/1578, CPE-1573, CPE-1708/CPE-1775, CPE-1821, CPE-1833/CPE-1836.
This shift then ADDS one row (the Trash surfaces below), so the header now reads **13**.

**Why a grep could not have done this — the ledger's tables were malformed in two ways, and both were
invisible in the raw text.**

1. A **blank line inside the Ledger** had detached rows #10–#14. With no header and no delimiter row of
   their own they were not a table at all — GitHub rendered them as a paragraph of pipe characters. Same
   class of break as the blank line that silently deleted a burndown row earlier this week.
2. **Row #3 was wrapped across ten physical lines.** In GFM each wrapped line renders as its own
   single-cell row, so the Ledger rendered nine spurious rows and row #3's real cells landed nowhere.

Also: three tables had no header/delimiter rows at all; `🟡 partial` was in use on row #6 but had never
been added to the Legend; and one CPE-1708 cell contains an **escaped** pipe (`\|`, a grep alternation)
that a naive `line.split("|")` reads as an extra column — GFM does not. All repaired, no cell text
changed. Verified by *rendering* with `marked` (gfm) as well as by parsing: 8 tables, Ledger = 14 body
rows, zero stray pipe-paragraphs, and the render's per-table row counts match the parser's exactly.

**The mechanism.** `src/lib/mvdLedger.ts` — a real GFM-aware parser (unescaped-pipe splitting, header +
delimiter + uniform column count required). Every table must be annotated
`<!-- mvd-table: primary|supplementary|excluded … -->`; an unannotated table is a hard failure, which is
also what makes a split table impossible to miss (the second half's nearest preceding non-blank line is a
table row, not an annotation). Each counted row must carry **exactly one** marker in **exactly one** cell.
`src/lib/mvdLedger.test.ts` recounts the real file and fails naming both numbers, plus 24 fixture cases.

**Every malformation reds; none of them shrinks the number.** Blank line, wrapped row, missing pipe,
stray unescaped pipe, absent delimiter row, unannotated table, no marker, markers in two cells, two
markers in one cell, no primary/supplementary table, missing header sentence, header stated twice, header
that does not add up — each has its own test.

**Counting rule, now documented** (Legend → "How the total is counted"): MVD = `⛰` + `🔧` + `🟡`; only
`✅` leaves it. The "6 primary" vs "4 primary manual" ambiguity the ticket names was exactly this, and it
is now stated in one place and enforced in another.

**Acceptance item 4 (Trash surfaces CPE-1822 did not photograph):** logged as a new supplementary row —
selection state + "N selected", the Empty/Restore `ConfirmDialog`, the `restoreErrors` banner, the
overflow menu, narrow width, long-filename truncation. Grep-confirmed absent from `trash.smoke.ts`;
all present in `TrashView.svelte`.

**Acceptance item 5 (`trash-degraded-scrolled` dark-only):** nothing to fix — **already resolved inside
CPE-1822**. The snapshot is named `trash-degraded-scrolled-dark`, the `it()` title says "dark theme only",
and the `snap()` call carries a comment explaining why. Recorded in the ledger so the question closes with
evidence rather than staying open.

**Carried discrepancy, stated rather than promised:** one row in an `excluded` historical table
(**CPE-1263**, the file-content search dialog) does not meet the exclusion's own criterion — its residual
is render/gui-smoke, not feel/taste, and it has a named retiring ticket, **CPE-1819**, live in
`Ticketing/Tickets/Backlog/` and named by Ledger row #12 as the shared blocker. By the file's own rule the
total is 13 where it is arguably 14. Not promoted in this shift for one reason: it would move the recount
off 12, the value PR #1042's UAT tester reached independently, trading the only external cross-check this
number has. The ledger now names the row, the ticket, the exact effect of promoting it (supplementary
7→8, total 13→14) and the reason it is deferred — a filed, assignable choice instead of a prose promise
about "the next pass".

**2026-08-27 — review round 2 (PR #1055, blocking finding fixed).**

`parseBurndown` gated table rows on `startsWith("|")`. **GFM allows up to three leading spaces**, so an
indented table was invisible to the parser while rendering identically. Reproduced on the real ledger by
indenting the 2026-08-10 supplementary table by two spaces: rendered page byte-identical (8 tables, rows
`[14,7,1,5,2,1,1,1]`), parser 13 → **10**, and *both* CPE-1932 floors still passed (`rows.length` 24→19 >
15; supplementary tables 5→4 > 2). The test then reds saying "Set it to the counted number above" —
instructing the next shift to write **10** into the header. A silent under-count laundered as verified:
precisely what this module's own header and the Legend declare impossible. Reachable by an ordinary edit
(nesting a debt table under a bullet).

Fixed: the gate is `/^ {0,3}\|/` and each row is trimmed before splitting. Four-plus spaces is a GFM
indented code block — **not** silently skipped either: `INDENTED_CODE_ROW` reds on it, since a row a human
meant as a table row and GFM renders as code is the same silent loss wearing a hat. Fenced code blocks are
now excluded from table scanning too (the non-blocking foot-gun: this ledger documents its own table
format, so a future shift quoting an example row inside a fence would have red "table is not annotated").

**Two floors added that would have caught it**, both red on the real file under the pre-fix gate:
`tables.length >= 8` ("a table disappeared from the parser's view … do not lower this floor"), and an
accounted-for check that counts table-shaped lines with a **looser** matcher than the parser's own gate
(`/^\s*\|/`) and asserts the parsed tables account for all of them — so if the gate ever narrows again the
loose count exceeds the accounted count and it reds instead of the number quietly shrinking. Verified:
`48 lines … look like table rows, but the parsed tables only account for 41`.

Fixtures added: 0/1/2/3-space indents all count identically; 4-space reds; a fenced pipe row is text.
36 tests (was 28). The Legend now documents the indentation rule, the fenced-block rule, and the known,
accepted limitation that the marker is pinned to "exactly one cell", not to the Status **column**.

**2026-08-27 — review round 3. The durable variant: a debt table that is never *gained*.**

Round 2's floors caught an existing table being *lost*. They did not catch a **new** table logged inside
a blockquote. Reproduced on the real ledger — GitHub renders 9 tables with all three `⛰` rows visible,
the parser's total does not move, and every check passes:

```
parser total    : 13  (header says 13 -> the header test PASSES)
tables parsed   : 8   | accounted lines: 48
floor A pre-fix : announced 8 vs parsed 8   -> PASS (blind)
floor B pre-fix : loose 48 vs accounted 48  -> PASS (blind)
```

Cause: floor B's loose matcher was `/^\s*\|/`, and `>` is not whitespace — so a blockquoted row was
rejected by the loose matcher **and** by the gate, and both counts moved together. Fix is one character,
`/^[\s>]*\|/`, applied to `TABLE_ANNOUNCEMENT` for the same reason. Same file, after:

```
floor A fixed   : announced 9 vs parsed 8   -> RED
floor B fixed   : loose 53 vs accounted 48  -> RED
```

**Floor A is now derived rather than hard-coded.** `tables.length >= 8` went slack the moment a ninth
table was added without bumping it — which is what made this variant durable. It now asserts
`tables.length === (number of <!-- mvd-table: … --> announcements)`, so adding a table raises the expected
count automatically and a table that stops being parsed while its announcement remains reds at once.

**One fence model, not two.** `fencedLines` is exported and the guard test reuses it. The test's own
simpler model toggled on any fence line and diverged from the parser on a ``` block containing a `~~~`
line — reddening a legal file with a message pointing at *tables* when the cause was a *fence*. While
there, the latent CommonMark divergence review flagged is **fixed, not just noted**: the closer must be
the same character **and at least as long**, so a four-backtick fence is no longer closed by a
three-backtick line (it would have counted a table GFM renders as code — an over-count, not a silent
loss, but wrong either way; a sibling PR hit the same class the same day).

**The 4-space decision was validated by measurement, better than it was argued.** At four spaces GFM
itself loses the table — 8 rendered tables become 7, the 5-row wave table gone — so a 4-space pipe row in
this file is *always* a mistake, and reddening it adds no false-positive surface (a genuine sample row
belongs in a fence, which is excluded outright).

40 tests, was 36. Fixtures added for the blockquote variant (all three floors, including a case pinning
that the pre-fix matcher passed), and for the fence-length rule.

**Interlock with CPE-1934 / PR #1052 (in flight).** Its `manual-test-mvd` ratchet entry reads this file's
header sentence with `\*\*MVD \(still-manual surfaces\):[^*]*?=\s*(\d+)\s*total\*\*`. That shape is
preserved byte-for-byte, and a dedicated test here asserts that regex still matches and still returns the
derived total, so the measurer keeps working whichever PR lands first. Its baseline of `16` becomes `13`,
which is a **lowering** — always legal, and the entry is `unenforced` anyway. Its `docs/design/RATCHETS.md`
enumeration table quotes `16`; whoever merges second should update that cell to `13`. No file is touched
by both PRs, so there is no merge conflict.
