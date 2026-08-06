---
id: CPE-1362
title: "PDF preview no longer displays — sandboxed iframe blocks WebView2's built-in PDF viewer"
type: Bug
status: Done
priority: High
component: Frontend
tags: [ready]
epic: CPE-059
created: 2026-08-06
closed: 2026-08-06
---

## Problem (reported live on v0.57.52-sidecar)

Opening a `.pdf` in the preview pane shows nothing — the right pane/view is blank. This is a
**regression** from the crash fix: PDFs rendered fine from CPE-062 until CPE-1357 landed.

## Root cause

CPE-1357 (#651) added `sandbox="allow-scripts allow-same-origin"` to the PDF `<iframe>`
(`src/lib/components/PreviewPane.svelte`) as defense-in-depth alongside the real crash fix (the
backend structural-validity gate + load-timeout/onerror fallback).

Chromium/WebView2 render PDFs through the **MimeHandlerView** plugin (the built-in PDF viewer). That
plugin **does not load inside a sandboxed iframe** — a `sandbox` attribute disables it — so the
iframe stays blank on a perfectly valid PDF. The real sample `samples/documents/doc.pdf` passes the
validity check (has `%PDF-` header, `startxref`, `/Count 3`), so it reaches the iframe and then
renders nothing.

## Fix

Remove the `sandbox` attribute from the PDF iframe. The crash protection CPE-1357 was for is fully
preserved without it:
- The **validity gate** (`loadPdfValidityFor` → backend `read_pdf_validity`) already prevents a
  malformed/empty PDF from ever being handed to the iframe — `pdfState` only reaches the render
  branch when the check passes.
- The **load timeout** + **`on:error`** handler still catch a valid-looking PDF that hangs the plugin.

The sandbox was the only new piece that also blocked *legitimate* rendering; dropping it restores PDF
preview while keeping the app crash-safe.

## Acceptance criteria

- Opening `samples/documents/doc.pdf` shows the rendered PDF in the preview pane.
- Opening `samples/documents/malformed.pdf` still falls back to the metadata slot (no crash) — the
  validity gate, not the sandbox, is what guards this.
- `npm run check` clean; existing `pdf_validity` backend tests still green.

## Work Log

- 2026-08-06 — Root-caused on v0.57.52-sidecar (reported live). The PDF `<iframe>` rendered fine from
  CPE-062 with no sandbox; CPE-1357 (#651) added `sandbox="allow-scripts allow-same-origin"`, which
  disables Chromium/WebView2's MimeHandlerView PDF plugin → blank pane on valid PDFs. Confirmed the
  real `samples/documents/doc.pdf` passes the validity gate (%PDF- + startxref + /Count 3), so it
  reaches the iframe and renders nothing.
- 2026-08-06 — Fix: removed the `sandbox` attribute (`src/lib/components/PreviewPane.svelte`). Crash
  safety preserved by the existing validity gate + load-timeout/on:error fallback. Updated
  `PreviewPane.pdf.test.ts` to assert the sandbox attr is now ABSENT. `npm run check` clean; 4/4 PDF
  component tests green. Shipped in v0.57.53-sidecar.
