---
id: CPE-1833
title: the status bar's advisory notes are never announced, and truncate into a title attribute only
type: bug
priority: Low
status: Doing
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

The status bar carries advisory sentences that change what the listing above it *means* — "N hidden by
filter", and (from CPE-1780) "N entries could not be read". Two accessibility gaps, both measured
during the CPE-1780 visual review:

1. **Never announced.** Neither `.statusbar` nor either note carries `role="status"`, `aria-live`, or
   any other live-region marker (measured: `barRole: null`, `barAriaLive: null` at every width). A
   screen-reader user is never told that what they are reading is filtered or incomplete — which is
   the whole point of the notes.

   **The naive fix does not work.** Each note is `{#if count > 0}`-conditionally mounted, i.e. it
   appears as a NEW element already containing its final text. That is precisely the shape that is
   frequently *not* announced, even with `role="status"` on the span itself. Chromium plus Windows AT
   — WebView2 with NVDA or Narrator, exactly this app — is the weakest pairing for it. The same lesson
   was learned on CPE-1816 the same day: a live region must exist *before* its content changes.

2. **The full sentence is reachable only by mouse.** At 684px and 600px both notes truncate
   (`isClipped: true`) and the complete text survives only in the `title` attribute. `title` is
   hover-only — not reliably exposed to keyboard-only or screen-reader users — so at a narrow window a
   low-vision or keyboard user has no path to the count at all. 600x400 is the app's own
   `.min_inner_size`, so this is a supported size, not an edge case.

## Acceptance criteria

- [x] A **persistent, always-mounted** container wraps the advisory region with `role="status"` /
      `aria-live="polite"`, and the notes change its text content rather than being inserted and
      removed. Verify the mid-change announcement actually happens, do not infer it from the markup.
- [x] Both notes are covered, and the case where **both apply at once** announces sensibly rather than
      as two competing sentences.
- [x] The full sentence is reachable without a mouse when the text is visually truncated — an
      accessible name that carries the untruncated text, or another affordance. `title` alone is not
      sufficient and must not be the only path.
- [ ] Verified with a real screen reader against the installed build. This is a behaviour of the
      assistive technology, not of the DOM — the markup already looks fine, which is the point.
      **Not done in this pass** — no `tauri-driver`/AT harness available in this environment (worker
      instructions explicitly forbid installing one). Left unchecked; see Work Log for the QA
      Architect hand-off.
- [x] Nothing regresses visually: the notes still truncate with an ellipsis and the row still fits at
      600px.

## Notes

Found by the Visual Critic during the CPE-1780 review, which classified both as follow-ups rather than
merge blockers — they are pre-existing for the filter note, and CPE-1780 doubles the surface rather
than introducing the gap.

Closely related and worth doing together if either is picked up: **CPE-1828** is the identical
mounted-with-its-text live-region defect in the Trash view's degraded notes. Same root cause, same
fix shape, same verification problem. Whoever takes one should take both.

The real-screen-reader verification step is a candidate row for the QA Architect's manual-test burndown
if it cannot be automated.

## Work Log (2026-08-23)

Worked together with CPE-1836 (same component, same PR) per the Foreman's assignment.

**Fix — a persistent, always-mounted announcer, separate from the two visible pills.** Added
`.advisory-live.sr-only` (`role="status" aria-live="polite" aria-atomic="true"`) right after the two
`{#if}`-conditional note spans in `StatusBar.svelte`. It is NEVER itself `{#if}`-gated — it exists at
every render, empty or not — only its text content changes, driven by a new reactive
`advisoryAnnouncement` that joins `filteredHiddenText`/`unreadableText` with `". "` when both are
present. `aria-atomic="true"` means a simultaneous change to both notes re-announces the WHOLE region
as one sentence, not two competing insertions. Visually hidden via the standard clip technique (never
`display:none`/`visibility:hidden`, which would drop it from the accessibility tree and reintroduce the
exact bug) — `position: absolute` also takes it fully out of the flex layout, so it costs zero width/gap
in `.statusbar` regardless of whether either note is present (verified: the non-busy baseline scenario's
measured rects were unchanged before/after, see CPE-1836's harness numbers below, which share this
component).

**Full text reachable without a mouse.** Added `tabindex="0"` to both `.filtered-hidden` and
`.unreadable` (with the same `a11y-no-noninteractive-tabindex` svelte-ignore precedent
`LogPreview.svelte`'s `.log-body` already established for this exact lint rule), plus a
`:focus-visible` CSS rule that reveals the untruncated text (`overflow: visible; white-space: normal`)
on focus. The element's own DOM text was already the FULL sentence — CSS `text-overflow: ellipsis` only
clips what's painted — so no separate `aria-label` was needed for the accessible name.

**RED-PROOF (asked for explicitly in the DoD).** Temporarily wrapped the announcer in
`{#if advisoryAnnouncement}` — the exact "naive fix" shape the ticket itself calls out as the trap
(brand-new element already holding its final text). Result: 4 tests in the new
`StatusBar.a11y.test.ts` went red immediately, starting with "the live region exists BEFORE any note is
present — not conditionally mounted" (which fails outright, `getByRole("status")` throws — the element
does not exist yet at that point). Reverted; suite green again. Pasted the key failing summary:

```
× the live region exists BEFORE any note is present — not conditionally mounted
× RED-PROOF: the SAME node updates its text in place when a note appears — proves it is not removed and re-inserted
× clears back to empty text (never removed) once both notes clear
× combines both notes' text in the single live region, marked aria-atomic
Test Files  1 failed (1)
     Tests  4 failed | 5 passed (9)
```

The strongest test (`RED-PROOF: the SAME node updates its text in place...`) captures the DOM node
reference BEFORE a note appears, then asserts the SAME node (`===`, not just equal content) carries the
text afterward — proves persistence, not just presence.

**Side effect, fixed:** the announcer duplicates the same sentence text as the visible pill, which broke
several PRE-EXISTING `getByText(exact sentence)` queries (ambiguous — two matching elements) across
`StatusBar.test.ts`, `App.filteredHiddenNote.test.ts`, and `App.statusBarCountStaleness.test.ts`. Fixed
by scoping those queries to `{ selector: ".filtered-hidden" }` / `{ selector: ".unreadable" }` (or
`:not(.sr-only)` for one regex-based absence check) — all now resolve to the visible pill specifically.
No behavioural change to what those tests assert, only disambiguation.

**Also required:** `src/lib/bidiEscape.guard.test.ts`'s line-number-keyed `REGISTRY["StatusBar.svelte"]`
needed updating (script/markup line numbers shifted, and the new `advisoryAnnouncement` expression is a
legitimate new entry — built purely from two already-registered-safe expressions, never a filesystem
path); and `src/app.css.test.ts`'s hard-coded-hex ratchet caught one new `#555` fallback I'd copied into
the `:focus-visible` box-shadow — removed the fallback (`var(--border-strong)` alone; the token is
defined in every theme block, no fallback needed).

**Assumption/judgment call:** real screen-reader verification (NVDA/Narrator against the installed
build) was NOT done — no `tauri-driver`/AT harness is available in this environment, and the worker
instructions explicitly forbid installing one. Flagging the PR for the accessibility leg / QA Architect
manual-test burndown per this ticket's own Notes section, which anticipated exactly this gap.

**Suite:** `npm run check` — 0 errors/warnings. Full frontend suite (`npx vitest run`) — 331 files,
4416 tests, all green (baseline before this branch: same file/test count minus the 2 new files added
here, all passing).
