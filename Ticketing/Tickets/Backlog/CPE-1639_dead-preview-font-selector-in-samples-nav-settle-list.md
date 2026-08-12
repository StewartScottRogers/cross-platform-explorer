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

- 2026-08-12 — Real-build verification pass on branch `cpe-1639-preview-font-selector` (PR #857, not
  merged). Also fixed two stale selector strings still named in `gui-smoke/README.md`'s prose
  (`.preview-font` and `.preview-media` — the latter never matched anything either; the real class has
  always been `.mp-media`, MediaPlayer.svelte). No functional change: `PREVIEW_CONTENT_SELECTOR` itself
  was already correct on `main`.

  **Evidence the corrected selector settles for real, under a real build (GitHub Actions
  `gui-smoke-linux`, ubuntu-latest, WebKitGTK/Xvfb, run
  https://github.com/StewartScottRogers/cross-platform-explorer/actions/runs/31593963928):**
  downloaded the job's raw protocol log (`gh api .../actions/jobs/94105007918/logs` — `gh run view --log`
  truncates on a run this size, ~17K vs ~76K lines; the raw jobs-logs endpoint doesn't) and traced worker
  `[0-29]` (the `samples.smoke.ts` worker — identified by its sequential per-file navigation: fonts →
  images → mail → …) through the `fonts/mini.ttf` case: at `12:27:21.397Z` it issues ONE
  `findElements` call for the full `PREVIEW_CONTENT_SELECTOR` list (now including
  `[data-testid="font-preview"]`) and gets a match back immediately (`RESULT [ {element} ]`, ~40ms round
  trip) — no retry loop. Contrast with the very next case in the same worker, `mail/*` (an `.eml`
  sample, one of the already-known-failing kinds, CPE-1507): the identical poll pattern instead cycles
  empty `[]` results every ~100ms for the full 20s window. Separately, worker `[0-26]`
  (`preview-pane.smoke.ts`'s own font spec, which carries its own independent
  `extraSelectors: ['[data-testid="font-preview"]']`) does a direct `findElement` +
  `getElementTagName` right after settling and gets back a real `div` — confirming the testid resolves
  to live, rendered DOM under this exact build, not just a string that happens to appear in source.

  **Evidence the corrected selector CAN fail (a selector that can't fail is what it replaced):** pushed a
  throwaway commit (`4a4a7a3e`, reverted the next commit, `15b76a12` — net diff against `main` is the
  README fix only) that pointed `PREVIEW_CONTENT_SELECTOR`'s font entry at a testid that matches nothing,
  and re-ran the same CI leg
  (https://github.com/StewartScottRogers/cross-platform-explorer/actions/runs/31598207125). Worker
  `[0-29]`'s `fonts/mini.ttf` case now shows the exact "stuck" `[]`-cycling pattern samples with genuinely
  broken previews show, then at `13:21:17.855Z` (20.8s after the poll started — the 20s timeout) mocha
  reports it explicitly:
  ```
  Error in "CPE-1358 — headless GUI smoke: every samples/ file opens without crashing the app.opens samples/fonts/mini.ttf: no crash + preview renders or gracefully degrades"
  Error: preview pane never settled for fonts/mini.ttf — still "Loading preview…" (or nothing rendered) after 20000ms
      at async waitForPreviewToSettle (.../gui-smoke/lib/samplesNav.ts:65:3)
      at async openAndVerify (.../gui-smoke/specs/samples.smoke.ts:101:3)
      at async Context.<anonymous> (.../gui-smoke/specs/samples.smoke.ts:128:7)
  ```
  As the ticket itself predicted, `preview-pane.smoke.ts`'s font spec (worker `[0-26]`) was UNAFFECTED by
  this break — it carries its own correct `extraSelectors` independent of the shared constant, so it kept
  passing throughout.

  **A ratchet-granularity gap worth flagging (not a defect in this fix):** `known-failing.json` and
  `gui-smoke/lib/ratchet.ts` operate at SPEC-FILE granularity, not per-`it()`. `samples.smoke.ts` is
  already listed known-failing for unrelated reasons (crypto/*, .eml/.ics/.vcf/.json — CPE-1507), so on
  BOTH the baseline run and the deliberately-broken run the ratchet reported the identical
  "41/41 spec(s) reported — 38 passed, 3 failed, 3 known-failing listed. OK" verdict — the job-level gate
  cannot see an individual case flipping within an already-failing spec file. That's why this
  verification had to read the raw per-test webdriver/mocha log rather than trust the ratchet's own
  output; it does NOT affect this ticket's fix (confirmed correct above) but is a latent blind spot: a
  regression in a currently-PASSING case inside `samples.smoke.ts` (fonts, images, PDFs, etc.) would not
  redden CI today. Filing this observation for a maintainer to decide whether it's worth a follow-up
  ticket (e.g. per-kind known-failing entries, or a dedicated assertion count check) rather than doing so
  unilaterally mid-verification.

  **Audit of the other 11 `PREVIEW_CONTENT_SELECTOR` entries:** re-confirmed the existing
  `gui-smoke/lib/samplesNav.test.ts` (from PR #837) still greps all 12 entries against
  `src/lib/components/*.svelte` and passes (35/35 gui-smoke unit tests green); did not find any other
  dead entry.

  Local verification: `npm run check` (svelte-check, 0 errors), root `npm test` (296 files / 3879 tests
  green), `gui-smoke && npm run test:unit` (35/35), `gui-smoke && npx tsc --noEmit` (clean). No Rust
  touched. PR #857 open, not merged; this ticket intentionally left open per assignment (verification-only
  hand-off, not a close-out).
