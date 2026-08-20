---
id: CPE-1819
title: The gui-smoke palette-open block is copy-pasted in three specs, and the one palette-only search dialog has never rendered in CI
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

Two burndown rows in `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` — primary row **#12** (AI content
search, shipped v0.57.45) and the supplementary **CPE-1263** row (`ContentIndexSearchDialog.svelte`, epic
CPE-976) — have both been parked since 2026-07-25 behind the same stated obstacle:

> "the dialog opens via the command palette rather than a free key combo, so a spec would need to drive the
> palette (`Ctrl+Shift+P` → type → Enter) rather than a direct key combo like `instant-search.smoke.ts`"

**That premise is obsolete, and has been for weeks.** Three specs already drive exactly that sequence against
the real `tauri build --no-bundle` binary, and all three are **green on the blocking WebKitGTK/Xvfb leg**
(none of them appears in `gui-smoke/known-failing.json`):

- `gui-smoke/specs/near-duplicates.smoke.ts:78-85`
- `gui-smoke/specs/similar-images.smoke.ts:73-80`
- `gui-smoke/specs/declutter.smoke.ts:100-107`

The block is copy-pasted **verbatim** in all three — same `browser.keys(["Control", "Shift", "P"])`, same
`$(".cp-input")`, same `waitForExist` timeout, same hand-written `timeoutMsg`. So there are two defects here,
not one:

1. **A duplicated harness idiom with no owner.** The next spec that needs a palette-opened dialog copies it a
   fourth time. When `CommandPalette.svelte`'s input class or the `commandPalette` chord in `keymap.ts`
   changes, four specs break independently and each gets fixed independently.
2. **Two MVD rows are held open by a blocker that no longer exists.** Row #12 has sat on `CHECKPOINT.md`'s
   "Owed to the USER" list for weeks on the strength of a stale note. The remaining work is not research —
   it is extracting a helper out of code that already works and writing one spec on top of it.

`tool.contentIndexSearch` (App.svelte:1142) is genuinely palette-only — unlike its sibling
`tool.searchInFiles` (App.svelte:1140) it carries no chord — so the palette really is its only opener, and
the dialog has therefore never been rendered by anything but a human.

## The honest limit — read this before writing the acceptance criteria

`ContentIndexSearchDialog`'s ranked-results leg (`.hit` / `.score-bar` / `.score-fill` / `.snippet`)
**cannot** be reached headlessly: those rows only exist once `content_index_build` has run, which needs a
live embedding endpoint (LM Studio / OpenAI) that CI does not have and must not acquire.

Do **not** paper over that with a mocked hit list — a `gui-smoke` spec asserting against a mock would be
exactly the "claim reading stronger than its evidence" defect this sprint spent itself finding. The
deterministic, falsifiable state on a fresh app process is the **needs-build offer** (`.offer` /
`.offer-title` / the `Build content index` primary button), which is the same off-means-off shape
`instant-search.smoke.ts` already pins for the Instant Search overlay, and for the same reason: a brand-new
app process is *guaranteed* into that state regardless of what is on the runner's disk.

The `query → results → navigate` loop row #12 asks for is still deliverable — just on the sibling **literal**
content search (`ContentSearchDialog.svelte`, `tool.searchInFiles`), which is a local backend search with no
model behind it and returns real hits against a seeded fixture. Pinning both dialogs in one spec file is what
makes this an M rather than an S.

## What to build

**1. `gui-smoke/lib/palette.ts`** — the one definition of "open the palette and run a command", in the same
spirit as `lib/theme.ts#setTheme` and `lib/paneWidth.ts#setPreviewPaneWidth`:

- `openPalette()` — clicks the breadcrumb first so focus is on a non-input element (App.svelte's
  `handleKeydown` ignores the chord while an INPUT/TEXTAREA has focus; `instant-search.smoke.ts:47-50`
  documents the identical precondition), sends `Control+Shift+P`, waits for `.cp-input`, and throws a named
  error if it never appears.
- `runPaletteCommand(query: string)` — `openPalette()`, `addValue(query)`, `Enter`, then asserts the palette
  actually closed, so "the command ran" is an assertion rather than an assumption.
- A header comment carrying the `Ctrl+Shift+P` → `keymap.ts`'s `commandPalette` `defaultChord` linkage, so a
  chord change has exactly one place to land.

**2. Refactor the three existing specs onto it.** They are the helper's proof: they are green today, so if
they are still green after the swap the helper is behaviour-identical. That is the cheapest available
regression test for a browser-driving helper that cannot be unit-tested.

**3. `gui-smoke/specs/content-search.smoke.ts`** — one new spec file (one, not two: `lib/shard.ts`'s header
measures 0.73–1.80 min of suite time per added spec file, and the shard partition is auto-computed from
`lib/specFiles.ts`, so a new file needs no workflow wiring but does cost a slice):

- *palette-only dialog:* `runPaletteCommand("search file contents")` → assert the dialog renders (its `h2` =
  `search.byContentTitle`, its `.q` input, its `.rebuild` button) and that the **needs-build offer** renders
  (`.offer-title` + the `Build content index` button) rather than a raw error — the exact claim
  `ContentIndexSearchDialog.test.ts` makes in jsdom, now made against the real binary over live IPC.
- `snap()` it in **both** themes via `lib/theme.ts#setTheme`. Dark-theme coverage is a standing gap this
  suite closed for the preview pane in CPE-1629 and must not re-open for new specs.
- Escape closes it.
- *sibling literal search, same file:* `runPaletteCommand("search in files")` → type a query matching a
  seeded fixture → assert a real `.group` / result row renders carrying the seeded filename → click it →
  assert the app navigated to that file. This is the `query → results → navigate` leg, delivered without a
  model.

**4. Update the burndown.** Flip row #12 to *render automated — live-embedding ranked-results residual* and
the CPE-1263 supplementary row likewise, naming the pinning job. Do not claim a full retire: say plainly
which sub-surface is still human-only and why.

## Acceptance criteria

1. `gui-smoke/lib/palette.ts` exists and exports `openPalette()` + `runPaletteCommand()`; `near-duplicates`,
   `similar-images` and `declutter` all import it and contain **no** remaining inline
   `browser.keys(["Control", "Shift", "P"])`. A repo-wide grep for that literal returns only `lib/palette.ts`.
2. `gui-smoke/specs/content-search.smoke.ts` exists and passes on a real local `npm run tauri build --
   --no-bundle` followed by `cd gui-smoke && npm test -- --spec ./specs/content-search.smoke.ts`, with the
   screenshots opened and actually looked at (the CPE-1629 precedent: "screenshots written" is not
   "screenshots verified").
3. The spec asserts, against the real binary: the palette-only dialog opens; its needs-build offer renders
   (not a raw error); Escape closes it; and the literal search's full query → results → navigate loop lands
   on the seeded file.
4. Both themes are snapped for the palette-only dialog (`setTheme("light")` and `setTheme("dark")`).
5. **The new spec is NOT added to `gui-smoke/known-failing.json`.** If it cannot pass on the Linux leg it is
   not done — a new entry in that file would make this ticket a no-op that reads like a win.
6. `gui-smoke-linux-verdict`'s ratchet reports the new expected spec count (`lib/specFiles.ts` computes it):
   confirm the verdict job's "how many should have reported" number moved by exactly one.
7. The burndown's row #12 and its CPE-1263 supplementary row are updated with the pinning job named and the
   honest residual stated.

## The pinning job

`GUI smoke (ubuntu-latest) shard N` (the 4-way matrix in `.github/workflows/gui-smoke.yml`) plus
`GUI smoke (ubuntu-latest) — verdict across all shards`, which owns the pass/fail verdict and is
**CI-blocking on `push` and `pull_request`** (CPE-1594 removed its `continue-on-error`, CPE-1753 sharded it,
CPE-1728 made a cancelled leg still produce a real verdict). Spec discovery is automatic via
`lib/specFiles.ts` — no workflow edit is needed, and none should be made.

Cross-OS note, stated so it is not mistaken for a hidden gap: `gui-smoke` has **no macOS leg** (no WKWebView
WebDriver in `tauri-driver`) and its Windows leg is `workflow_dispatch`/nightly-only (CPE-1048, the WebView2
`DevToolsActivePort` crash). Linux is the blocking leg and is where this is pinned. That is the same bar
every other ✅ row in the burndown was flipped on.

## Evidence

Per the Evidence Rules in `Ticketing/wiki.md`. Two red-proofs, both required, because this ticket makes two
independent claims:

- **The helper is behaviour-identical.** Break `lib/palette.ts` (e.g. send `Control+P`), confirm all four
  specs — the three refactored ones and the new one — go red, then restore. A helper exercised only by the
  new spec has not been proven against the specs it replaced.
- **The new assertions can fail.** Rename `.offer-title` in `ContentIndexSearchDialog.svelte` and confirm the
  needs-build assertion reds; separately, delete the seeded fixture's matching content and confirm the
  literal-search results assertion reds. Two separate deletions — one red does not prove two assertions.

State explicitly in the work log what each assertion fails *for*. "The suite is green" is not evidence that a
new spec asserts anything.

## Notes

Filed by the **QA Architect** on the 2026-08-20 shift, out of the audit that found the stale premise. MVD
this shift: **15 → 16** (supplementary CPE-1098 flipped ✅ as already-pinned by CPE-1173; +2 new rows for the
Trash degraded-listing states and the StatusBar advisory lines).

Related: **CPE-1263** (the dialog's jsdom coverage), **CPE-1594** (the ratchet that made the Linux leg
blocking), **CPE-1753** (sharding — read `lib/shard.ts`'s header before adding a spec file), **CPE-1629**
(the `lib/theme.ts` dark-theme helper this spec reuses), **CPE-1143** (`instant-search.smoke.ts`, the
off-means-off assertion shape this copies).

Runner-up this shift, deliberately not filed: burndown row **#14** (AI Console sidecar UI in a real browser).
It is described there as "mostly transcription" of `sidecar/agent-board/clickthrough.mjs` — but that harness
is **still not wired into any CI job**, so transcribing it would produce a second local-only script and could
not flip a row to ✅ under this ledger's own bar. The CI-wiring problem has to be solved first, and that is a
different ticket from the transcription.
