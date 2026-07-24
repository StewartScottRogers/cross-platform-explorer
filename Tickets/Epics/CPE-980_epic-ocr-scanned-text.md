---
id: CPE-980
title: "EPIC: OCR & scanned-document text"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-24
closed:
---

## Goal
Make scanned documents and images with text **first-class**: extract the words from a scanned PDF, a photo
of a receipt, or a screenshot, so that text becomes searchable, previewable, and copyable — just like a born-
digital document. "Find the receipt that says 'Acme Hardware'" should work even when the file is a JPEG.

## Why
A huge slice of real-world files are image-only: scans, receipts, screenshots, photos of documents. Today
`doc_text`/`content_search` see nothing in them, so they're invisible to search and preview. OCR closes that
gap and is a force-multiplier for the other new epics: it feeds text into semantic search ([[CPE-976]]),
smart folders ([[CPE-978]]), and auto-organize ([[CPE-979]]). Delivered as an opt-in, feature-gated capability
so the lean core pulls in no OCR engine unless enabled.

## Rough scope (areas, not child tickets)
- An **OCR provider seam** (`trait OcrEngine`): pluggable — a bundled lightweight engine and/or an opt-in
  external service — feature-gated OFF so the plain build has zero OCR weight (lean-core, fast-when-off).
- **Pipeline integration**: route image/scanned-PDF bytes through OCR to produce text that flows into the
  existing `content_search` / `doc_text` / preview paths and (later) the semantic index.
- A **text-layer cache**: persist extracted text keyed by content hash (reuse the content-addressed store,
  [[CPE-969]]) so a page is OCR'd once, not per search; incremental on change.
- **Preview + copy**: show recognised text alongside the image; select/copy; highlight search hits on the page.

## Open questions (resolve at activation — big-design)
- **OCR backend:** bundle an engine (binary size, per-OS build, languages) vs. an opt-in external service vs.
  both — the central big-design call, weighed against the fast/small tiebreaker.
- Trigger policy: on-demand (open/preview) vs. background-on-index; cost/perf budget for large scan sets.
- Language/script coverage; quality vs. speed knobs; how to represent word boxes for hit-highlighting.
- Privacy for an external engine (never send content off-device without explicit opt-in).

## Definition of Done
- Text in scanned PDFs and images is extracted and made searchable + previewable + copyable.
- Extraction is cached (OCR once per content), incremental, and opt-in; disabling it removes all OCR weight.
- OCR'd text feeds the same search/preview surfaces as born-digital text.

## Notes
- Enabler for [[CPE-976]] / [[CPE-978]] / [[CPE-979]] on image-only files. Build the **provider seam + the
  text-cache + pipeline wiring** headless first (a `FakeOcr` in tests proves the flow with zero engine
  weight, mirroring `provider::FakeProvider`), then integrate a real engine behind the feature. See
  [[headless-frontier-and-cpe-net]].
