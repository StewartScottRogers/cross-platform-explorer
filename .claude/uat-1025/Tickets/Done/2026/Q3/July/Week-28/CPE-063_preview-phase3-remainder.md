---
id: CPE-063
title: Preview pane Phase 3b — markdown render, syntax highlighting, archive listing, Office
type: Feature
status: Done
priority: Low
component: Multiple
estimate: 4h+
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Remaining preview providers from [[CPE-059]] Phase 3 that were deliberately NOT done in [[CPE-062]]
because each carries a decision or a capability that shouldn't be shipped unverified on the Nightshift
machine (no cargo; no pixel/GUI observation; bundle-size + XSS-sandbox judgement calls).

## Items (split into child tickets when worked)

- **Markdown rendering** — ✅ Done in [[CPE-065]] (marked → DOMPurify, sanitized; script/handler
  stripping unit-tested).
- **Syntax highlighting** — ✅ Done in [[CPE-065]] (highlight.js core + curated languages).
- **Archive listing** — ✅ Done in [[CPE-064]] (ZIP; Rust `read_archive_entries` + `archive` provider),
  verified locally after Rust was installed on the machine. Other formats (7z/rar/tar) remain optional.
- **Office / OpenDocument** — **Declined (documented decision).** A real Office/ODF preview needs a
  heavy parser/renderer (docx/xlsx/pptx are zipped XML; rendering fidelity is a project of its own) with
  no safe lightweight option. These files fall back to the metadata pane, which is adequate. Revisit
  only if there is concrete demand.
- **List-view thumbnails** — **Deferred (documented decision).** Per-row image thumbnails in the list
  view are a separate performance-sensitive enhancement (decode/caching, visual verification) distinct
  from the single-selection preview pane this epic delivered. Track separately if wanted.

## Acceptance Criteria

- [ ] Each item is delivered (or explicitly declined with a documented reason) as its own child ticket
- [ ] Markdown/HTML output is sanitized and CSP-safe; verified in the running app
- [ ] Any Rust (archives) lands green via CI on a PR
- [ ] `npm run check` clean; JS suite green; Rust suite green in CI

## Resolution

Archive listing (CPE-064) and markdown rendering + syntax highlighting (CPE-065) delivered, verified,
and merged. Office/OpenDocument preview **declined** and list-view thumbnails **deferred** — documented
above as deliberate decisions (metadata fallback covers Office; thumbnails are a separate perf-sensitive
enhancement). All actionable items are now resolved, so the [[CPE-059]] epic can close.

## Notes

The high-value preview types (image/text/media/pdf/json/csv) work as of CPE-060/061/062; this ticket
covered the remaining code/markdown/archive richness plus the Office/thumbnail decisions.
