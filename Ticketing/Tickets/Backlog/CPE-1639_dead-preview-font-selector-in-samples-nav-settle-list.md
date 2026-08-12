---
id: CPE-1639
title: "gui-smoke's PREVIEW_CONTENT_SELECTOR carries a dead \".preview-font\" class — the font case settles by luck, not by the selector meant to catch it"
type: Bug
status: Backlog
priority: Medium
component: Testing
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the independent Reviewer verifying PR #827 (CPE-1629, gui-smoke preview-pane screenshot specs)
against a real Windows build. While stress-testing `gui-smoke/lib/samplesNav.ts` under realistic load
(~a dozen concurrent cargo/rustc/node processes from other sprint workers, ~66% CPU), a standalone
`specs/samples.smoke.ts` run hit ~15 settle-timeouts. All but one matched the already-documented
CPE-1507 known-failing set (`crypto/*`, `.ics`, `.vcf`). The outlier was `fonts/mini.ttf`.

## The gap
`PREVIEW_CONTENT_SELECTOR` (`gui-smoke/lib/samplesNav.ts`, extracted byte-identical from
`specs/samples.smoke.ts` by CPE-1629) includes `.preview-font` as one of the "definite content"
selectors `waitForPreviewToSettle` polls for:

    export const PREVIEW_CONTENT_SELECTOR = [
      ".preview-img", ".mp-media", ".preview-pdf", ".preview-font", ".preview-table-wrap",
      ".preview-markdown", ".code-view", "pre.preview-text", ".preview-editor",
      '[data-testid="hexview"]', ".data-browser", "aside.details",
    ].join(", ");

`.preview-font` is not a real CSS class anywhere in the current frontend. `grep -rn "preview-font" src/`
finds it in exactly one place, `FontPreview.svelte`'s dynamically generated `@font-face` family name
(`` `preview-font-${mine}` ``) — a JS string, never applied as an element's `class`. The component's
actual root is `<div class="font-preview" data-testid="font-preview">` (note: `font-preview`, not
`preview-font` — the two words are swapped). So for the entire life of this selector list, the
`fonts/*` case in `waitForPreviewToSettle` has never matched on `.preview-font`; it has only ever
"passed" via the loop's other exit condition (`.preview-note` reaching a non-loading terminal state, or
one of the other 11 selectors incidentally matching first if the DOM order/timing lines up). That's a
settle condition that can be satisfied by an unrelated coincidence rather than by the thing it claims to
detect — exactly the shape of "passes when the machine is fast/idle, times out when it's loaded," which
is what the Reviewer's stress run reproduced.

`specs/preview-pane.smoke.ts` (CPE-1629) is NOT affected in practice: its font test passes its own
working `extraSelectors: ['[data-testid="font-preview"]']` and separately asserts on that real testid
directly, so it never depended on the dead `.preview-font` entry. But `specs/samples.smoke.ts` — the
suite `PREVIEW_CONTENT_SELECTOR` was written for — has no such extra selector for the `fonts/` case, so
it depends entirely on the dead entry (plus luck).

## Fix
Change `.preview-font` to `.font-preview` (or `[data-testid="font-preview"]`, matching the testid
convention the rest of the list already leans on for hexview/data-browser) in
`gui-smoke/lib/samplesNav.ts`'s `PREVIEW_CONTENT_SELECTOR`.

Per samplesNav.ts's own header comment, this constant is deliberately byte-identical to what
`samples.smoke.ts` measured its `known-failing.json` entry (CPE-1507) against — changing it changes
what that spec considers "settled" for the `fonts/mini.ttf` case, so re-verify `samples.smoke.ts`
against a real build afterward (ideally under load, matching how this bug was found) and update the
CPE-1507 known-failing reason/verification if the fonts case's behavior under that entry changes.

## Acceptance criteria
- `PREVIEW_CONTENT_SELECTOR` in `gui-smoke/lib/samplesNav.ts` no longer contains a selector matching
  zero elements anywhere in the shipped frontend — audit the other 11 entries for the same class of bug
  while in there (this ticket found one by accident, not by a systematic check).
- `specs/samples.smoke.ts`'s `fonts/mini.ttf` case settles on the corrected, real selector — verified by
  a real build run, not just by the test passing (a coincidental pass looks identical to a real one).
- No change to `specs/preview-pane.smoke.ts`'s own behavior (it doesn't depend on this entry).

**Conflict surface:** `gui-smoke/lib/samplesNav.ts` only (one exported constant). Touches the same file
CPE-1629 just extracted; sequence after that PR lands to avoid a rebase collision on the same lines.

## Work Log
2026-08-11 — Claim confirmed: `.preview-font` matches zero elements anywhere in `src/lib/components`;
`FontPreview.svelte`'s real root is `<div class="font-preview" data-testid="font-preview">`. Fixed by
changing the entry to `[data-testid="font-preview"]` (matching the testid convention already used for
hexview). Audited the other 11 entries per the acceptance criteria — all match real elements (verified
by grep against `src/lib/components/*.svelte`: `.preview-img`, `.mp-media`, `.preview-pdf`,
`.preview-table-wrap`, `.preview-markdown`, `.code-view`, `pre.preview-text`, `.preview-editor`,
`[data-testid="hexview"]`, `.data-browser`, `aside.details` all real). Added
`gui-smoke/lib/samplesNav.test.ts` (new, runs via `tsx --test` under `npm run test:unit` — no build or
tauri-driver needed) that locks in the fix and systematically greps every `PREVIEW_CONTENT_SELECTOR`
entry against the shipped frontend so the same class of bug can't silently reappear for a different
selector; confirmed it fails red on the old `.preview-font` entry and passes green on the fix. `gui-smoke
npm run test:unit`: 35/35 green (32 pre-existing + 3 new).

**Not done in this batch:** the ticket's fix note also calls for re-verifying `samples.smoke.ts`'s
`fonts/mini.ttf` case against a **real build** (ideally under load) once this selector fix lands, and
updating the CPE-1507 known-failing entry if the fonts case's behavior changes. That needs a full
`tauri build` + `tauri-driver`/WebdriverIO session, which is out of scope for this headless three-ticket
batch — left as a follow-up. The selector fix itself and the systematic audit are done and verified by
the new unit test. Batched with CPE-1620 and CPE-1622 into PR #837 (branch
`cpe-1620-1622-1639-small-fixes`).

- 2026-08-11 — The code fix (corrected gui-smoke settle selector) shipped in PR #837. Ticket stays open:
  its remaining acceptance bullet needs a real `tauri build` + tauri-driver run to confirm the
  `fonts/mini.ttf` case settles on the corrected selector under load. Reviewer + UAT both confirmed the
  selector now matches real markup (`FontPreview.svelte` `[data-testid="font-preview"]`) and that no other
  gui-smoke consumer regressed.
