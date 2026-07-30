---
id: CPE-1144
title: "QA: pin the Batch-Media dialog render in gui-smoke (burn down CPE-1093 residual)"
type: chore
component: Testing
priority: low
status: Done
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
- [x] `gui-smoke/specs/batch-media.smoke.ts` drives the real build to the Batch-Media dialog, adds an op, and
      asserts the op-pill list + plan preview render non-empty; non-destructive (no Apply / no image writes
      outside the throwaway tmpDir).
- [x] Fixture seeds valid image(s) into the disposable tmpDir; cleaned up by `onComplete` (no residue).
- [x] Any new `data-testid`/test-mode seam is inert in production; existing smoke specs still pass; `npm run
      check` passes.
- [x] Wired non-blocking (`continue-on-error`); the real gui-smoke suite passes locally incl. the new spec.

## Work Log
- Studied the proven pattern (`organize.smoke.ts` / `instant-search.smoke.ts`) and the surface:
  `BatchMediaDialog.svelte` (op builder + debounced `batchMediaPlan` preview), `batchMedia.ts`
  (`canBatchTransform`/`partitionEligible` — encoder-writable extensions only), and `App.svelte`'s
  opener. **No command-palette entry exists for Batch Media** (grep of `paletteCommands` — no
  `batch-media` id, unlike `tool.organize`); the only real opener is the right-click context menu's
  "Batch media…" item (`ContextMenu.svelte`, gated on `selectionCount > 1 && mediaEligible`).
- **Fixture (`wdio.conf.ts#seedBatchMediaFixture`)**: `batch_media::plan` (crates/server/src/batch_media.rs)
  turned out to be pure path-string manipulation — it never opens/decodes the file for the preview — but
  the ticket explicitly asked for real decodable images so the fixture stays honest with what a real
  batch-media flow encounters (and keeps working if `plan` ever starts inspecting bytes). Hand-rolled a
  minimal valid truecolour PNG encoder (IHDR/IDAT via `node:zlib` deflate/IEND, CRC32 per the PNG spec) —
  no external deps — and self-validated it (CRC recompute + zlib inflate round-trip on every chunk) before
  wiring it in. Two 4x4 PNGs (`CPE-1144-photo-a.png` / `-b.png`) are seeded into the same disposable tmpDir
  as every other fixture; cleaned up by the existing `onComplete` `rm -rf`.
- **Spec (`batch-media.smoke.ts`)**: selects both seeded rows (plain click + ctrl+click), right-clicks to
  open the context menu (preserving the multi-selection, mirroring `App.svelte`'s `onRowContext`), clicks
  "Batch media…" (found by scanning `.ctx .row` HTML for its label, same technique organize.smoke.ts uses
  for palette rows), adds the default Resize(1024px) op via `[data-testid="add-op-btn"]`, then asserts
  `[data-testid="op-pill"]` renders "Resize 1024px" and `[data-testid="preview-row"]` renders exactly 2 rows
  named `CPE-1144-photo-{a,b}-1024.png` (the backend's actual computed output names — falsifiable, tied to
  this spec's own fixture). Dismissed via `[data-testid="cancel-btn"]` — Apply is never clicked.
  - Two DOM gestures with no prior precedent in this harness — ctrl+click and right-click — are driven via
    `browser.execute` dispatching a real `MouseEvent` at the row's own DOM node (Svelte's handlers just read
    `e.ctrlKey`/event type off whatever `MouseEvent` they receive), rather than relying on WebDriver's
    native pointer/actions API against wry's embedded WebView2 control under the classic-protocol
    workaround this harness already forces.
- **Seam added**: four inert `data-testid`s in `BatchMediaDialog.svelte` (`add-op-btn`, `op-pill`,
  `plan-preview`/`preview-row`, `cancel-btn`/`apply-btn`) — same minimal-hook pattern CPE-1142 used for
  `OrganizeDialog.svelte`. No `--test-mode`-gated hook was needed; the existing right-click opener was
  reachable headlessly.
- **Verification — ran the real gui-smoke suite locally** (`npm run build` → `npm run tauri build --
  --no-bundle` → `cd gui-smoke && npm ci && npm test` with
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--disable-gpu --no-sandbox --disable-dev-shm-usage"`): **6 spec
  files / 9 tests, all passing**, exit code 0 — batch-media.smoke.ts's 1 test included. `npm run check`:
  0 errors, 0 warnings.

## Notes
- Flips the burndown CPE-1093 row from "logic automated — pixel/theme residual" to "render pinned by
  `gui-smoke` (CPE-1144)". This is the **last** seedable GUI surface with render debt — after it, remaining
  manual-test debt (cost-ledger / radar live tabs) needs a live agent session, not an on-disk fixture.
- This machine has tauri-driver + msedgedriver — run the real gui-smoke locally to prove the spec green.
