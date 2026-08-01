---
id: CPE-1241
title: "QA: gui-smoke pin + Visual Critic screenshot for the ShredConfirmDialog (CPE-1240)"
type: Task
priority: Medium
component: gui-smoke
tags: [ready]
created: 2026-08-01
epic: CPE-738
closed: 2026-08-01
status: Done
---

## Context
CPE-1240 (#539) shipped the secure-delete confirm dialog (ShredConfirmDialog) — a PERMANENT, no-undo
destructive surface. Reviewer + UAT verified its honesty copy + safeguard + conventions at code level;
it needs a gui-smoke screenshot for the Visual Critic (the dialog's rendered clarity/tone matters for a
destructive op).

## Repro to drive
Right-click a file (not folder/archive/Home) → "Securely delete…" → the ShredConfirmDialog opens with
the permanence warning + platform caveat + scheme picker + "Shred permanently" danger button. Do NOT
click Shred (would permanently destroy the seeded fixture) — snap the dialog, then Cancel/Escape.

## Acceptance criteria
- New `gui-smoke/specs/shred-dialog.smoke.ts` (mirror near-duplicates/saved-search specs) drives the real
  built app to OPEN the ShredConfirmDialog on a seeded throwaway file, asserts the permanence + caveat
  copy + danger button render, `snap("shred-dialog")`s it, then dismisses via Cancel (NEVER confirms —
  no real shred).
- Spec passes green + captures `shred-dialog.png`. Visual Critic judges: clearly reads as dangerous/
  permanent, honest tone, visible border, danger button distinct, nothing clipped, on-theme.

## Notes
QA-Architect burndown for the new destructive surface; mirrors CPE-1221/1233. CRITICAL: the spec must
never actually confirm the shred.

## Work Log
- 2026-08-01 — Added `gui-smoke/wdio.conf.ts#seedShredFixture`: a DEDICATED subfolder
  (`CPE-1241-shred-folder`) with one throwaway file (`CPE-1241-shred-me.txt`), isolated from every
  other spec's fixtures so this spec's blast radius, if anything ever went wrong, is contained to one
  file (belt-and-braces on top of the "never click confirm" rule).
- Added `gui-smoke/specs/shred-dialog.smoke.ts`, mirroring `new-link.smoke.ts` / `macro-in-menu.smoke.ts`'s
  structure: navigates into the seeded subfolder (double-click via a `.row` HTML-scan locator, same
  primitive every spec in this suite uses), real-right-clicks the seeded file row via the CDP
  `rightClick` helper (`lib/mouse.ts`, CPE-1155 — non-grabbing, real hit-testing), scans the resulting
  `.ctx` menu's `button.row` elements for "Securely delete…" (`ContextMenu.svelte`'s `shreddable`-gated
  row) and clicks it.
- Asserts against `ShredConfirmDialog.svelte`'s real selectors: `[aria-label="Securely delete?"]` (the
  dialog's static aria-label), `[data-testid="shred-permanence"]` (asserts the text matches
  `/permanent/i` and `/non-recoverable/i`), `[data-testid="shred-caveat"]` (asserts `/best-effort/i`),
  `[data-testid="shred-scheme"]` (the overwrite-scheme picker exists), `[data-testid="shred-confirm"]`
  (asserts the danger button exists and is labelled "Shred permanently" — **never clicked**), and
  `[data-testid="shred-cancel"]` (exists, and IS clicked to dismiss).
- CRITICAL SAFETY: the spec never clicks `shred-confirm`. It clicks `shred-cancel`, waits for the
  dialog to unmount, then asserts `fs.existsSync(seededFilePath)` is still `true` — a falsifiable,
  on-disk proof that nothing was shredded. Also asserts the file exists BEFORE the click-through, so
  the after-check is meaningful.
- `snap("shred-dialog")` captures the fully-rendered dialog (permanence copy, caveat box, scheme
  picker, Cancel + danger button) right before the Cancel click; `afterEach` calls
  `snapFailure(this.currentTest, "shred-dialog")` per CPE-1149.
- Built the real app in this worktree (`npm run build && npm run tauri build -- --no-bundle`, release
  profile, `src-tauri/target/release/cross-platform-explorer.exe`) and ran the spec against it:
  `npx wdio run ./wdio.conf.ts --spec shred-dialog` -> **2 passing (14.6s)**. Captured
  `gui-smoke/.screenshots/shred-dialog.png`; no `-fail.png` produced. Visually reviewed the
  screenshot: red trash icon + "Securely delete "CPE-1241-shred-me.txt"?" header, bold
  permanent/non-recoverable copy, bordered best-effort caveat box (SSD/copy-on-write text fully
  legible, nothing clipped), scheme dropdown showing "Zero-fill — 1 pass — fast", Cancel (neutral) +
  "Shred permanently" (solid red, clearly distinct) side by side, visible dialog border, on-theme —
  reads clearly as dangerous/permanent with an honest tone.
- Verification: `gui-smoke` typecheck (`tsc --noEmit -p tsconfig.json`) clean. No product/src change —
  `ShredConfirmDialog.svelte` / `ContextMenu.svelte` / `App.svelte` are untouched; this is
  gui-smoke-only.
