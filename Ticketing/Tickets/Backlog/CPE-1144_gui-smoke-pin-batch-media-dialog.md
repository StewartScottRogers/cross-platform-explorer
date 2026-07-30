---
id: CPE-1144
title: "QA: pin the Batch-Media dialog render in gui-smoke (burn down CPE-1093 residual)"
type: chore
component: Testing
priority: low
status: Backlog
tags: ready
created: 2026-07-30
epic: CPE-723
---

## Summary
QA-Architect pass. The Batch-Media dialog (CPE-1093/1105, epic CPE-723) has its **logic** automated
(`BatchMediaDialog.test.ts`, jsdom) but its **real-build render** is only human-verified ("pixel/theme feel
residual" on the burndown). It's the last seedable-from-disk GUI surface not yet pinned by `gui-smoke`
(code-preview/replay/cost-history/instant-search/organize are all pinned; the cost-ledger CPE-1098 + radar
CPE-1100 tabs are live-IPC-fed and genuinely NOT seedable from disk, so they stay human-glance). Add a
`gui-smoke` render pin so the dialog can't silently break on the real build.

## Design (mirror the proven pattern — `gui-smoke/specs/organize.smoke.ts` / `instant-search.smoke.ts`)
- Seed a tmpDir with a couple of **real, decodable image files** (the dialog plans image transforms, so it
  needs valid images — write a minimal valid PNG/JPEG in `wdio.conf.ts#seedBatchMediaFixture`, or reuse any
  existing image fixture the repo has; keep it tiny). Cleaned up by the existing `onComplete` tmpDir `rm -rf`.
- `gui-smoke/specs/batch-media.smoke.ts`: reach the dialog the same way it's opened in the app (find the
  opener — command palette entry / Tools or context menu; grep `BatchMediaDialog`/`batchMedia` in `App.svelte`
  + `MenuBar.svelte`/command-palette registrations). Select the seeded image(s) if the flow needs a selection.
  Add an operation (e.g. resize/convert) via its control, and assert the **op-pill list + the debounced plan
  preview render** (stable selectors from `BatchMediaDialog.svelte` — grep its markup for `.bm-*`/`data-testid`
  hooks; if none exist, add minimal inert `data-testid`s like CPE-1142 did for OrganizeDialog).
- **Non-destructive:** do NOT click Apply/Execute (no actual image writes) — assert the *preview* only, then
  Cancel. If a selection seam is genuinely unreachable headlessly, add the smallest `--test-mode`-gated hook
  (mirror existing ones); prefer existing openers.
- Non-blocking CI signal (`continue-on-error`, CPE-1048), auto-discovered by the `specs` glob — no workflow
  edit needed.

## Acceptance Criteria
- [ ] `gui-smoke/specs/batch-media.smoke.ts` drives the real build to the Batch-Media dialog, adds an op, and
      asserts the op-pill list + plan preview render non-empty; non-destructive (no Apply / no image writes
      outside the throwaway tmpDir).
- [ ] Fixture seeds valid image(s) into the disposable tmpDir; cleaned up by `onComplete` (no residue).
- [ ] Any new `data-testid`/test-mode seam is inert in production; existing smoke specs still pass; `npm run
      check` passes.
- [ ] Wired non-blocking (`continue-on-error`); the real gui-smoke suite passes locally incl. the new spec.

## Notes
- Flips the burndown CPE-1093 row from "logic automated — pixel/theme residual" to "render pinned by
  `gui-smoke` (CPE-1144)". This is the **last** seedable GUI surface with render debt — after it, remaining
  manual-test debt (cost-ledger / radar live tabs) needs a live agent session, not an on-disk fixture.
- This machine has tauri-driver + msedgedriver — run the real gui-smoke locally to prove the spec green.
