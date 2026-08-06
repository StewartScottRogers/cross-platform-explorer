---
id: CPE-1363
title: "Harden archive inner-preview cache + PDF load-timeout (adversarial-audit follow-ups)"
type: Bug
status: Done
priority: Low
component: Frontend
tags: [ready]
epic: CPE-059
created: 2026-08-06
closed: 2026-08-06
---

## Problem

An adversarial bug-audit of this session's new preview code (CPE-1360 archive inner-file preview +
CPE-1362 PDF handling) surfaced three genuine low-severity defects. No crash, but real edge-case bugs
worth fixing while the code is fresh.

1. **Archive-preview cache never re-validated (Low-Med).** `archivePreview.ts extractInnerToTemp` returned
   a cached temp path without checking it still exists. If the OS (Disk Cleanup / Storage Sense) or the
   user clears `%TEMP%` mid-session, re-selecting that inner file returns a dead path and the preview
   fails "can't display" until app restart — the cache actively defeats the re-extract that would fix it.
2. **PDF load-timeout leaked on PDF -> non-PDF navigation (Low).** The `$: if (… kind === "pdf")
   loadPdfValidityFor` reactive only cleared the 15s `pdfLoadTimer` / bumped `pdfReqId` when the *new*
   entry was a PDF. Navigating from a still-loading PDF to a non-PDF left a dangling timer that fired
   `pdfState = "error"` 15s later (invisible, but a real leak + stray mutation).
3. **Cache key ambiguity (Very Low / theoretical).** Reported as a space-joined key that could collide.
   On inspection the separator was already a raw NUL byte (unambiguous) — but a raw NUL embedded in a
   `.ts` source is fragile; replaced with the proper `"\0"` escape.

## Fix

- `src/lib/archivePreview.ts`: on a cache hit, re-validate via a cheap `entry_info` stat (`stillExists`);
  if the temp file is gone, drop the entry and re-extract to the stable temp path. Factored the extractor
  dispatch into a private `extract()`. Cache key now uses an explicit `"\0"` escape.
- `src/lib/components/PreviewPane.svelte`: added an idempotent `cancelPdfLoad()` (clears the timer,
  supersedes the pending validity check via `pdfReqId`, resets to idle) and gave the PDF reactive an
  `else` branch that calls it when the selection leaves a PDF.

## Tests

- `archivePreview.test.ts`: cache-hit-with-live-temp still served from cache (counts `extract_archive_entry`
  calls, not total invokes, since re-validation now also calls `entry_info`); NEW reaped-temp case where
  `entry_info` throws forces a re-extract.
- `PreviewPane.pdf.test.ts`: NEW fake-timer test — PDF -> non-PDF before the iframe settles disarms the
  timeout so it can't flip state 15s later.
- Full JS suite green; `npm run check` 0 errors.

## Notes

Rust half of the same audit (rar_extract_entry, pdf_validity) came back **clean** — no panics / bounds /
mis-routing; only a documented approximate page-count heuristic that never affects the validity gate.

## Work Log

- 2026-08-06 — Two parallel adversarial auditors (Rust + frontend) reviewed the session's new code. Rust
  clean. Frontend: 3 low-sev findings above, all fixed + covered by tests. Self-verified (14/14 preview
  tests, check clean). Audit-driven hardening; shipped in the next build.
