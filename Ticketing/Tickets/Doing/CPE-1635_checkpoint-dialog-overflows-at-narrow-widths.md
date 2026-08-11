---
id: CPE-1635
title: "The Checkpoints dialog overflows the viewport at narrow window widths, despite max-width:95vw"
type: Bug
status: Doing
priority: Low
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Seen by the Visual Critic in real (headless) Chrome while reviewing CPE-1600 (PR #826). **Pre-existing** —
the critic explicitly re-checked with the ordinary content scenario, no long strings involved, and
reproduced it identically, so CPE-1600's new failure rows are not the cause and did not worsen it.

## The gap
At a **420px** viewport width, `CheckpointDialog`'s own chrome — the header buttons and Close — pushes past
the viewport edge, defying the `max-width: 95vw` the dialog declares. The content area behaves; it is the
dialog's own header row that fails to shrink or wrap.

Worth fixing because Checkpoints is a **recovery** surface. A user reaching for it is usually already having
a bad day, and a control they cannot reach because it is off-screen is a bad time to discover a layout bug.

## Fix
- Make the header row shrink or wrap sensibly instead of forcing the dialog wider than its own `max-width`.
  The usual culprits: a `flex` row whose children have no `min-width: 0`, or a button group with
  `flex-shrink: 0` and no wrapping.
- Check the other dialogs for the same shape while you are there — if this pattern was copied, it is copied
  elsewhere. Report what you find rather than silently fixing a dozen files; that may deserve its own ticket.
- **Verify by looking in a real browser at narrow widths, in both themes.** jsdom cannot see layout — this
  defect existed precisely because nothing that runs in CI can see it. If CPE-1629's screenshot harness has
  landed by then, consider adding a narrow-width capture of this dialog so it cannot regress unseen.

## Acceptance criteria
- At 420px (and narrower), the dialog and all its controls stay within the viewport; no horizontal scroll.
- Real checkpoints and failed-attempt rows both still read correctly at that width, in both themes.
- Screenshot evidence in the work log, since no automated test asserts this today.

**Conflict surface:** `src/lib/components/CheckpointDialog.svelte` (and possibly sibling dialogs if the
pattern is shared). Small and self-contained.

## Work Log (2026-08-11)

### Harness built for real verification
jsdom can't do layout, so built a small, reusable, standalone Vite dev harness (not the app's own dev
server): `scripts/dev-harness/checkpoint-narrow/` + `vite.harness.config.ts` (repo root; run via
`npm run harness:checkpoint-narrow`). It mounts the **real** `CheckpointDialog.svelte` with its two
backend-talking imports (`../invoke`, `../bindings.gen`) aliased to canned mocks
(`scripts/dev-harness/checkpoint-narrow/mocks/`) supplying two real checkpoints (one with a label, one
without) and one CPE-1600 failed-attempt row, loads the app's real `src/app.css`, and drives real Chrome
against it headlessly via Bash (per this session's hazard notes: never PowerShell `Start-Process` for
this — it mangles `&` in URLs — and never `taskkill /IM chrome.exe` — kills the user's own browser too;
used `--user-data-dir` scoped to a scratch profile and let each one-shot `--screenshot` process exit on
its own).

**A second pitfall beyond the one this session already knew about.** The known trap is `--window-size`
not reliably setting the CSS viewport under `--headless=new`. Working around that with a large real
window (1200×900) plus a CSS-sized wrapper `<div>` turned out to be **insufficient on its own**: `vw`
units (and the `.backdrop`'s `position:fixed`) resolve against the true top-level browser viewport no
matter how a descendant element is sized or `transform`-anchored — a wrapper `<div>` can't change what
`100vw` means. An early harness version measured `.dialog` rendering at its full 680px declared width
inside a "424px-wide" wrapper, because 95vw of the real 1184px Chrome window is ~1125px — nowhere near
constraining. **Fix: host the mounted dialog inside an `<iframe>`**, sized via the outer page's CSS —
an iframe is a genuinely separate browsing context with its own real viewport at exactly its box size,
so `vw`/`position:fixed` inside it behave exactly like a real narrow OS window. `index.html` (outer
shell, sizes the iframe) + `inner.html`/`inner-main.ts` (mounts the dialog inside it) implement this;
`outer-main.ts` reads back `getBoundingClientRect()`/`innerWidth` diagnostics from the iframe's own
`contentWindow` (same-origin) and writes them into a visible on-page readout, so both a screenshot and a
`--dump-dom` text extraction can confirm the **achieved** width before trusting anything visual.

### Root cause: could NOT reproduce the overflow with a properly verified viewport
With the iframe technique confirming a genuine, exact CSS viewport (`innerWidth` reads exactly the
requested value every time), the dialog's header (`.head-row`: title + docs button) and footer
(`.actions`: Close) **never overflowed the viewport** at 420px, 360px, or all the way down to 300px,
with the ordinary two-checkpoints-plus-one-failure content the ticket describes. `.dialog` consistently
rendered at exactly `min(680, 0.95×viewport)` as declared, centered, with `document.scrollWidth` equal
to the viewport width (`HAS_HORIZONTAL_OVERFLOW=false`) throughout. Screenshots at 420px/360px/300px,
light and dark, all confirm this — no clipped or off-screen control at any width tested.

Pushed further: mounted the dialog with an **artificially long title** ("Checkpoint & rollback for a
very long example project name that keeps going") at 320px to actively try to provoke the reported
symptom. **Negative control** (current `main`, before any change here): the long h2 just **wraps onto
3 lines** — Chrome's default flex behavior lets a block-level h2 shrink to its min-content (the widest
single word) and wrap the rest, growing the row taller rather than wider. It never overflowed
horizontally either. So even the stress case doesn't reproduce a viewport-overflow bug — it reproduces
an *ugly 3-line title*, a different (milder) symptom than the ticket describes.

**Strong direct evidence for what actually happened**: reproduced the *exact-looking* false positive
using the naive methodology (bare `chrome --headless=new --window-size=420,900`, no iframe, no
verified-viewport check) against the same unmodified markup/CSS/content — the resulting screenshot shows
the Refresh button, both Revert… buttons, and the Close button all visibly cut off past the right edge
of the 420px-tall PNG, looking identical in kind to the ticket's reported symptom. This is the headless
Chrome viewport-clamping artifact this session's other hard-won guidance names explicitly ("a critic
nearly filed a false defect over this") — the internal render happens at a wider, clamped viewport and
the screenshot gets rescaled down into the requested frame, cutting off whatever didn't fit the
requested pixel count. It is very likely this ticket's original report is exactly that artifact, not a
real CSS defect in the shipped dialog.

**Conclusion:** could not confirm the "before" state this ticket describes despite rigorous, repeated,
verified-viewport testing (420 down to 300px, ordinary AND artificially stressed content, both themes).

### Fix applied anyway (defensive, not a confirmed-bug fix)
Even without a reproduced defect, `.head-row`'s `h2` genuinely lacked `min-width: 0` (a flex item's
`min-width: auto` floors it at its own min-content size — the exact "usual culprit" the ticket named),
and the negative-control screenshot above shows the real, if milder, consequence: a long title wraps to
3 lines rather than degrading gracefully. Applied the ticket's suggested fix shape as low-risk
hardening in `src/lib/components/CheckpointDialog.svelte`:
- `h2 { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }` — the title now
  truncates to one line with an ellipsis under squeeze, instead of wrapping to multiple lines.
- `.docs { flex-shrink: 0; }` — the inverse guarantee: the icon button stays fixed at 26×26 rather than
  also shrinking and distorting its SVG once the row is squeezed enough for the ellipsis to engage.

Re-ran the same stress scenario (long title, 320px, both themes) with the fix in place: the title now
reads "Checkpoint & rollback for a …" on one line, `.head-row`/`.dialog`/`.actions` all still report
`OVERFLOWS_VIEWPORT=false` (unchanged — they never overflowed), and ordinary-content screenshots at
420px are pixel-identical to before the change (no regression). Only the multi-line-title symptom
changed, from wrap-to-3-lines to single-line-ellipsis.

**Theme colours**: no hard-coded hex added; the two new rules are pure layout properties
(`min-width`/`overflow`/`text-overflow`/`white-space`/`flex-shrink`), no colour tokens touched at all —
consistent with "never change a shared global token to fix one dialog."

### Screenshot evidence (all captured this session, real Chrome, verified achieved viewport)
- 420px light, ordinary content, before fix: no overflow (`.dialog` right=409.5 vs viewport 420, all
  `OVERFLOWS_VIEWPORT=false`).
- 420px light, ordinary content, after fix: pixel-identical to before.
- 420px dark, ordinary content, after fix: no overflow, readable.
- 360px / 300px light, ordinary content, before fix: still no overflow (buttons/list rows shrink and
  ellipsis-truncate correctly).
- 320px light + dark, artificially long title, before fix (negative control): title wraps to 3 lines,
  no horizontal overflow.
- 320px light + dark, artificially long title, after fix: title truncates to one line with ellipsis, no
  horizontal overflow.
- Naive `--window-size=420,900` (no iframe, no verified viewport) against the SAME unmodified code:
  produces a screenshot with Refresh/Revert…/Close visibly cut off at the right edge — the false-positive
  artifact, captured side-by-side with the correctly-verified negative result for comparison.
(Screenshots are session-local under the harness's scratch dir, not committed — the harness itself,
which reproduces all of the above on demand, is committed.)

### Sibling dialogs — SAME `.head-row` class + CSS shape (report only, not fixed here)
Grepped every `src/lib/components/*Dialog*.svelte` for the `.head-row { display:flex; ...
justify-content:space-between ...}` + bare `h2 { font-size:16px; }` shape (no `min-width:0` on the
title). Three more share it verbatim, strongly suggesting a copied pattern:
- `OrganizeDialog.svelte` — `.head-row { display:flex; align-items:center; justify-content:space-between;
  gap:8px; margin-bottom:10px; }` + `h2 { font-size:16px; }`, dialog `max-width:95vw`.
- `ColumnPickerDialog.svelte` — identical shape, dialog `max-width:95vw`.
- `CopilotDialog.svelte` — `.head-row { display:flex; align-items:center; gap:8px; margin-bottom:4px; }`
  + `h2 { font-size:16px; flex:1; }` (has `flex:1`, but `flex:1` shorthand sets `flex-basis:0`, not
  `min-width:0` — the `min-width:auto` floor still applies), dialog `max-width:94vw`.

None of these were verified for an actual overflow (that would need the same iframe-harness treatment
per dialog, out of this ticket's scope) — flagging per the ticket's own instruction to report rather
than silently fix a dozen files. Given CheckpointDialog itself didn't reproduce a real defect, these
three are lower-confidence candidates for a shared "hardening pass" ticket, not confirmed bugs.
`MetadataStudioDialog.svelte` and `InspectCryptoDialog.svelte` also have `h2` header rows but use a
different class name/structure — not checked further.

### Tests
- New `src/lib/components/CheckpointDialog.narrowWidth.test.ts` (4 tests, jsdom): parses the component's
  raw `<style>` source (same convention as `src/app.css.test.ts`) and pins the fix's mechanism —
  `h2`'s `min-width:0`/`nowrap`/`ellipsis` trio and `.docs`'s `flex-shrink:0` — so a future edit can't
  silently drop them. Explicitly documented in the test file that jsdom cannot verify the actual layout
  claim; that's what the harness + screenshots above are for.
- `npx vitest run`: **280 files / 3426 tests, all passing** (18 in the two CheckpointDialog test files:
  14 existing + 4 new).
- `npm run check`: **0 errors, 0 warnings.**

### Recommendation
Given the "before" state could not be reproduced with a rigorously verified viewport, and a plausible,
directly-demonstrated alternate explanation exists (headless-Chrome `--window-size` clamping artifact),
suggest treating this as: defensive hardening shipped (title truncation), no confirmed viewport-overflow
bug found in `CheckpointDialog.svelte` today. Left in **Doing** (not Done) for a human call on whether to
close this out as resolved-by-hardening or reopen/retest against the real packaged app if the report
recurs. The harness (`npm run harness:checkpoint-narrow`) is reusable for that retest, or for auditing
the three sibling dialogs above.
