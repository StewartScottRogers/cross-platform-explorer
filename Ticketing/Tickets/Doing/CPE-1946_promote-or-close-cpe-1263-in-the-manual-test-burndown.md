---
id: CPE-1946
title: promote-or-close CPE-1263 in the manual-test burndown — by the file's own rule it is countable debt, so the total is 13 and arguably 14
type: task
priority: Low
status: In Progress
tags: ready
estimate: XS
created: 2026-08-27
---

## Summary

`.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` carries one deliberately-recorded discrepancy, put
there by CPE-1922 so that a decision would be **filed and assignable** rather than left as a prose
promise about "the next pass".

The ledger's `excluded` annotation excludes the 2026-07 historical tables because their residual is
**feel/taste** — *"work that has no retiring ticket."* One row in that table does not meet that
criterion:

**CPE-1263** (the file-content search dialog, `ContentIndexSearchDialog.svelte`). Its residual is
**render/gui-smoke**, not feel/taste, and it **does** have a named retiring ticket: **CPE-1819**
— *"The gui-smoke palette-open block is copy-pasted in three specs, and the one palette-only search
dialog has never rendered in CI"* — live in `Ticketing/Tickets/Backlog/`, `tags: ready`. Ledger row
#12 names CPE-1819 as the shared blocker and says solving it retires the CPE-1263 residual too.

By the rule the file states, that is countable debt.

## Why it was not simply counted

Counting it during CPE-1922 would have moved the recount off **12** — the value PR #1042's UAT
tester reached **independently**, by a different method (bare grep vs a GFM parser). That
cross-check is the only external verification the number has, and trading it to fold in one row was
not worth it. The decision was deferred; the record of it was not.

## The arithmetic, if promoted

    primary        6  ->  6   (unchanged)
    supplementary  7  ->  8
    total         13  -> 14

## Acceptance criteria

- [x] **Decide: promote or close.** Promote if the render/gui-smoke residual is real manual debt that
      CPE-1819 will retire. Close if it turns out CPE-1819 already covers it in a way that means it
      was never separately countable, or if the residual has since been automated.
- [x] If promoted: move the row out of the `excluded` table into the supplementary ledger, update the
      header total to **14**, and delete the discrepancy note and its short form in the `excluded`
      annotation. The derived-count test from CPE-1922 will fail until the header matches, which is
      the point.
- [x] If closed: say why in the ledger, against the criterion — not "decided not to count it".
- [x] Either way, **remove both halves of the note** so the ledger stops carrying a known
      discrepancy. A recorded discrepancy is a good interim state and a bad permanent one.
- [x] Check whether any **other** row in the excluded tables fails the same criterion. CPE-1263 was
      found by reading the annotation against its own rows; nobody has checked the rest. Enumerate
      (CPE-1932).

## Notes

Filed 2026-08-27 by the sprint Foreman, off wording PR #1055's Reviewer verified claim-by-claim
against CPE-1819's frontmatter and Ledger row #12's cells.

Related: **CPE-1922** (the recount that surfaced it, PR #1055), **CPE-1819** (the retiring ticket),
**CPE-1263** (the residual itself), **CPE-1042** (the independent UAT recount to 12 that the
deferral protects).

## Work Log

**2026-08-27 — decided: PROMOTE.** Argued against the annotation's own criterion, not against
convenience.

1. **The residual is not feel/taste.** CPE-1263's Status cell read *“logic automated (jsdom) —
   render/gui-smoke + pixel/feel residual”*. Its pixel/feel half genuinely does meet the exclusion
   criterion (the score-bar fill needs real hits, which need a live embedding endpoint). Its
   **render/gui-smoke half does not** — `ContentIndexSearchDialog.svelte` has never rendered in CI at
   all. Per the file's own “How the total is counted”, a row is MVD whenever a human still has to look
   at something for the row's claim to hold; an unburnable *sub*-residual does not excuse a burnable
   one, or every `🟡 partial` row in the primary Ledger would be excluded on the same reasoning.
2. **It has a named retiring ticket** — the exact fact the criterion turns on. CPE-1819 re-read off its
   frontmatter this shift: `Ticketing/Tickets/Backlog/`, `tags: ready`, `estimate: M`, `status:
   Backlog`. No other row in either `excluded` table names a ticket.
3. **Neither close-path holds.** *“CPE-1819 already covers it so it was never separately countable”* —
   no: Ledger row #12 is the **AI search** dialog (v0.57.45), CPE-1263 is **`ContentIndexSearchDialog`**
   (epic CPE-976). They share a *blocker*, not a *surface*; CPE-1819 retires both, which is why row #12
   is itself counted. *“The residual has since been automated”* — no: `gui-smoke/lib/` has no
   `palette.ts`, and none of the 43 specs in `gui-smoke/specs/` reaches the dialog (listed this shift).

**Result: primary 6 → 6, supplementary 7 → 8, total 13 → 14.** The 12 PR #1042's UAT tester reached
independently was a check on the *recount method*; CPE-1922 spent it, and it was never a ceiling on
what the tables may contain.

**Enumeration of the rest of both `excluded` tables** (CPE-1932 — read row by row, not recalled).
Seven other rows; every one passes the criterion, so nothing else is promoted:

| Row | Status cell | Residual | Retiring ticket | Verdict |
|-----|-------------|----------|-----------------|---------|
| CPE-1090 | `automated — pinned by gui-smoke (CPE-1096)` | fold-animation / jump *feel* | none | excluded, correctly |
| CPE-1091 | `automated — pinned by gui-smoke (CPE-1096)` | fold-animation / minimap-scrub *feel* | none | excluded, correctly |
| CPE-1093 | `**render automated** — pixel/feel residual` | pixel/*feel* on the installed build | none | excluded, correctly |
| CPE-1094 | `render **automated** — feel residual` | *feel* | none | excluded, correctly |
| CPE-1098 | `**render automated** — live-numbers/feel residual` (blocking-pinned) | live token/USD numbers off a real **paid** agent run + card *feel* | none | excluded, correctly — attended observation of a paid external run, nothing CI can pin |
| CPE-1100 | `render **automated** — feel residual` | *feel* | none | excluded, correctly |
| CPE-1114 | `**automated — pinned by gui-smoke (CPE-1130)**` (2nd `excluded` table, its only row) | none named | none | excluded, correctly |

CPE-1263 was the **only** row in either table carrying a *render* residual, and the **only** one naming
a retiring ticket. The annotation's blanket claim (“every row now reads render automated — feel
residual”) was true of seven rows out of eight.

**Changes**

- `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md`
  - header sentence `6 primary + 7 supplementary = 13 total` → `6 primary + 8 supplementary = 14 total`;
  - the CPE-1263 row **moved verbatim** out of the 2026-07-26 `excluded` table into a new
    `<!-- mvd-table: supplementary -->` table (Surface + “Automated coverage today” cells byte-identical,
    reshaped to the foot tables' 6 columns, Status restated as the Legend marker `🔧 in progress`);
  - **both halves of the recorded discrepancy deleted** — the two-paragraph note in the CPE-1922 section
    (replaced by a one-line history pointer saying the question was decided here) and the parenthetical
    inside the 2026-07-26 `excluded` annotation (replaced by the row-by-row audit result above);
  - new `## Promoted 2026-08-27 (CPE-1946)` section carrying the argument and the enumeration.
- `docs/design/RATCHETS.md` — the `manual-test-mvd` enumeration row's hardcoded current value 13 → 14.
  That baseline is `unenforced: true`, so the rise prints a note rather than reddening `ratchet-guard`
  (the reason is enumerated at its registration: MVD legitimately rises when an audit finds unlogged
  debt, and gating it would push audits toward not logging what they find).

**Red-proof.** With the row moved and the header left at 13, `npx vitest run src/lib/mvdLedger.test.ts`
fails 2/40: *“header (line 7) claims 6 primary + 7 supplementary = 13 total, but a fresh count of its own
tables says 6 primary + 8 supplementary = 14 total”*, plus the CPE-1934 measurer-shape case. With the
header at 14: **40/40 green**, and the parser's own derived breakdown reads `11 manual, 2 in progress,
1 partial; 11 rows retired`. `src/lib/ratchetBaselines.test.ts` 67/67 green alongside it.

**File hygiene.** No PowerShell touched any repo file. The ledger stays UTF-8, **no BOM**, **CRLF**
throughout (`file` + `xxd` checked before and after every write); the row move was done by a Node script
that splits and re-joins on `\r\n` explicitly, so no line ending was rewritten.
