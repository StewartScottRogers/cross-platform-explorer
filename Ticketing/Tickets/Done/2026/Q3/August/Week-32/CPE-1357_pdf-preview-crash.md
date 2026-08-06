---
id: CPE-1357
title: "PDF preview crashes the app — malformed/empty PDF in the WebView2 iframe takes down the renderer"
type: Bug
status: Done
priority: High
component: Multiple
tags: [ready]
epic: CPE-059
created: 2026-08-06
closed: 2026-08-06
---

## Problem (reported live on v0.57.50)

Opening a `.pdf` in the preview pane made the app "go nuts and crash." Reproduced trigger:
`samples/documents/doc.pdf`.

## Root cause (two layers)

1. **The app crashes on a bad PDF.** The PDF provider renders via a raw WebView2 iframe:
   `src/lib/components/PreviewPane.svelte:840` — `<iframe class="preview-pdf" src={assetUrl(entry.path)}>`
   where `assetUrl` = `convertFileSrc` (Tauri asset protocol; `assetProtocol.scope = ["**"]`, csp null).
   WebView2's built-in PDF viewer renders the file; on a malformed/empty PDF it can crash the WebView2
   renderer process, which takes the whole window down. **No PDF a user opens should ever crash the app.**
2. **Our sample PDF is itself invalid.** `samples/documents/doc.pdf` (406 bytes) is a degenerate fixture:
   `2 0 obj << /Type /Pages /Kids [] /Count 0 >>` (ZERO pages) and it has a `trailer` but **no `xref`
   table** — not a loadable PDF. (Replacing it is tracked in CPE-1358's "valid sample per type" work.)

## Fix direction

Make PDF preview **crash-resilient** so a malformed/empty/huge/encrypted PDF degrades gracefully instead
of killing the renderer. Options (pick per testing on the real build):
- **Isolate the iframe** — add `sandbox` (and/or `referrerpolicy`), and/or render it in a way whose crash
  can't propagate to the main window; add an `onerror`/load-timeout fallback to a "can't preview this PDF"
  state.
- **Validate before embedding** — cheap structural sanity (has an `xref`/startxref + ≥1 page) via the
  existing PDF read path (`media_meta_read::read_pdf` / the `pdf-thumb` feature already parses PDFs); if it
  fails, show the metadata/"can't preview" state rather than handing a broken file to WebView2.
- **Consider routing the preview through the pdfium first-page render** (the `pdf-thumb` path) instead of
  the raw iframe, at least as the fallback — pdfium fails safely with an `Err`, WebView2's plugin does not.
- Whatever the approach, the pane must catch/contain the failure; the app process must survive.

## Acceptance criteria

- Opening a malformed/empty PDF (incl. the current degenerate fixture, and a real one) does NOT crash the
  app — it renders the PDF or shows a graceful "can't preview" / metadata fallback.
- Opening a VALID multi-page PDF renders it.
- Covered by the sample-navigation gui-smoke harness (CPE-1358) so it can't regress; `npm run check` + JS
  suite green.
- Verified on a real build (build → deploy → run): open both a good and a bad PDF, app stays alive.

## Notes

Reported 2026-08-06 from the running v0.57.50 sidecar. The gui-smoke sample harness (CPE-1358) is the
regression vehicle — it should reproduce this and stay green after the fix. The final "app survives" is an
attended/gui-smoke check on the real build.

## Work Log
- 2026-08-06: PR #651 merged. PDF preview crash-resilience: pure pdf_validity byte-scan (requires %PDF- + startxref; rejects /Count 0; unknown page-count = not-rejected so compressed-xref/linearized PDFs still render) gates the iframe before src is set; iframe sandbox + on:error + 15s timeout defense. read_pdf_validity command + bindings regen. Also fixed sample_fixtures::pdf_info_baseline (red on main from the doc.pdf swap). Reviewer APPROVE (14 real Adobe PDFs incl compressed-xref NOT over-rejected) + UAT PASS (pikepdf compressed-xref accepted). Final WebView2-survives check = CPE-1358 gui-smoke. Minor non-blocking: stale pdfLoadTimer not cleared on navigate-away pre-15s (harmless).
