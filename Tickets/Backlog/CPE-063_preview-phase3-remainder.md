---
id: CPE-063
title: Preview pane Phase 3b — markdown render, syntax highlighting, archive listing, Office
type: Feature
status: Open
priority: Low
component: Multiple
estimate: 4h+
created: 2026-07-11
closed:
---

## Summary

Remaining preview providers from [[CPE-059]] Phase 3 that were deliberately NOT done in [[CPE-062]]
because each carries a decision or a capability that shouldn't be shipped unverified on the Nightshift
machine (no cargo; no pixel/GUI observation; bundle-size + XSS-sandbox judgement calls).

## Items (split into child tickets when worked)

- **Markdown rendering** — render `.md` to HTML. MUST sanitize (a markdown file can embed HTML/script);
  the CSP/sandbox needs live verification, so this is not safe to ship blind. Choose a lib
  (bundle-size-conscious, no external requests) + a sanitizer.
- **Syntax highlighting** — highlight code previews (currently raw text). Pick a highlighter
  (highlight.js / Shiki) mindful of bundle size and the no-CDN constraint.
- **Archive listing** — list zip entries without extracting. Needs a Rust command (+ a crate) → land via
  a PR so CI compiles/tests it, as CPE-061 did.
- **Office / OpenDocument** — investigate; likely metadata-only or a heavy dependency. Decide and
  document rather than force a preview.

## Acceptance Criteria

- [ ] Each item is delivered (or explicitly declined with a documented reason) as its own child ticket
- [ ] Markdown/HTML output is sanitized and CSP-safe; verified in the running app
- [ ] Any Rust (archives) lands green via CI on a PR
- [ ] `npm run check` clean; JS suite green; Rust suite green in CI

## Notes

Blocks final closure of the [[CPE-059]] epic. Lower priority than shipping — the high-value preview
types (image/text/media/pdf/json/csv) already work as of CPE-060/061/062.
