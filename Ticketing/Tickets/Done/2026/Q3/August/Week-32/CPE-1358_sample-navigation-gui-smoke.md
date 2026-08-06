---
id: CPE-1358
title: "QA: gui-smoke that opens EVERY sample file (no crash + preview renders), + a valid sample per supported type, run in CI"
type: Task
status: Done
priority: High
component: Multiple
tags: [ready]
epic: CPE-1148
created: 2026-08-06
closed: 2026-08-06
---

## Goal (QA Architect mission — erode Manual Verification Debt)

An automated regression that **launches the real app and navigates through every file in `samples/`**,
asserting the app does not crash and each file's preview renders (or degrades gracefully). Plus: ensure a
**valid sample fixture exists for every file type/category the app supports**, so this harness covers the
whole surface. Run it **regularly in CI**. This directly targets the class of bug just found manually —
the PDF preview crash (CPE-1357) — which an automated sample walk would have caught.

## Why

Previews are a huge manual-test surface (open each type, eyeball it). The user's standing goal is to never
test by hand. A sample-navigation smoke turns "open every kind of file and see if it breaks" into a CI gate.
It already paid for itself before it exists: opening `samples/documents/doc.pdf` crashed the app, and that
`doc.pdf` is itself a degenerate 0-page/no-xref fixture.

## Scope

1. **Valid sample per supported type (audit + fill).** Enumerate the app's supported categories/preview
   kinds (`src/lib/filetypes.ts` `categoryOf` + `src/lib/preview/provider.ts` kinds: image, decoded-image,
   raw-image, heic, dicom, audio, video, pdf, json/csv/tsv, archive, markdown, text, info/hex, data-grid,
   font, 3D-model, …). Audit `samples/` for a **valid** fixture per category; **fill the gaps** and
   **replace invalid ones** — notably **replace `samples/documents/doc.pdf`** (the degenerate 0-page/no-xref
   file) with a real, valid multi-page PDF; add any missing (e.g. a `.dcm`, `.cr2`, `.heic`, `.rar`,
   `.stl`/`.obj`, `.ttf`, `.sqlite`/`.xlsx`, a `.svg`, a `.tiff`/`.psd`, a font, etc.). The just-added
   HEIC/DICOM/RAW/RAR samples (CPE-1341-1351) count. Keep fixtures tiny.
   - Add a **headless unit/vitest** guard asserting the sample set covers every supported category (so a new
     format without a sample fails CI) — a cheap coverage ratchet independent of the heavier gui-smoke.
2. **gui-smoke navigation spec.** Following the existing `gui-smoke/specs/*.smoke.ts` + `wdio.conf.ts`
   (tauri-driver + WebdriverIO on the real built binary): open the app pointed at `samples/`, then for EACH
   sample file: select it, wait for the preview pane, and assert (a) the app process is **still alive / the
   window still responds** (the crash check — this is the CPE-1357 regression), and (b) the preview rendered
   *something* for its kind (an `<img>`/iframe/data-grid/entry-list/text/etc., or an explicit graceful
   "can't preview" state — never a hard crash or an infinite spinner). Drive it data-first off the sample
   tree so new samples are covered automatically.
3. **Run in CI regularly.** Wire the spec into the existing GUI-smoke workflow (`.github/workflows/gui-smoke.yml`)
   so it runs on the 3-OS-capable legs it supports (Windows/Linux; macOS gui-smoke is unsupported per prior
   notes). Keep it a required-ish gate for preview regressions. Note the Windows HEIC sample needs the OS
   HEIF extension — the harness must treat a documented "codec absent → graceful metadata fallback" as a
   PASS, not a crash.

## Acceptance criteria

- A valid, tiny sample exists for every supported preview category; the coverage guard test passes and fails
  when a category lacks a sample. `doc.pdf` replaced with a real valid PDF.
- The gui-smoke spec opens every `samples/` file on a real build and asserts no-crash + a rendered/graceful
  preview for each; it reproduces the CPE-1357 PDF crash on today's code (and passes once CPE-1357 lands).
- The spec runs in CI (gui-smoke workflow); `npm run check` + JS suite green.
- MVD burndown row added/updated in `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` for "open each file
  type by hand" → automated.

## Notes

QA Architect owns the design; a Worker implements the harness. Pairs with CPE-1357 (the harness is that
bug's regression test). Epic CPE-1148 (Visual-Critic / gui-smoke). Filed 2026-08-06 (user-requested).

## Work Log
- 2026-08-06: PR #652 merged. QA sample-navigation harness: sampleCoverage.test.ts (self-updating ratchet derived from provider.ts; fails if a preview kind lacks a sample) + gui-smoke/specs/samples.smoke.ts (opens every sample on the real app, asserts alive+rendered/graceful, malformed.pdf last as CPE-1357 regression pin) wired via wdio specs glob. Filled coverage: tiff/tsv/zip/ttf/sqlite/wasm; kept degenerate PDF as malformed.pdf. Reviewer CHANGES-then-APPROVE (merged-state pdf_info_baseline reconciled to script-gen doc.pdf; aside.details selector fix for the #651 fallback) + UAT PASS (ratchet mutation-killed, fixtures real-parser-verified). Surfaced CPE-1359 (rar not in provider.ts ARCHIVE_EXT).
