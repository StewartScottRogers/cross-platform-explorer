---
id: CPE-1448
title: "Doc-preview '(truncated)' marker can be silently swallowed (docx mid-tag strip; content-search 4MiB outer cap)"
type: Bug
status: Backlog
priority: Low
component: Backend
tags: [ready]
epic: CPE-724
created: 2026-08-07
---
## Observation (from CPE-1446 review + UAT of PR #710)
CPE-1446 caps zip-entry decompression at 8MiB and appends a `"… (truncated)"` marker when the cap is hit.
The memory bound is solid, but the MARKER (a cosmetic honesty cue) can disappear in two edge cases:
1. **docx/odt/xlsx mid-tag strip** — `zip_read_text`/`zip_read_text_optional` call `mark_truncated` on the
   RAW XML, then `strip_markup_to_text` (a naive `<`→in_tag / `>`→out scanner) strips tags. If the 8MiB cut
   lands inside an open tag with no following `>`, `in_tag` stays true to end-of-string and the appended
   marker is stripped away with it → truncated preview, no visible marker. (`pptx_text`/`epub_text` avoid
   this by marking AFTER stripping.)
2. **content-search 4MiB outer cap** — `content_text.rs`'s pre-existing `MAX_EXTRACTED_CHARS = 4MiB`
   (CPE-1274) runs AFTER doc_text's new 8MiB inner cap; since 4MiB < 8MiB it trims the string further and
   cuts off the trailing marker before search ever sees it. So a bomb doc is silently (not visibly)
   truncated on the search path.

Neither weakens the DoS fix (memory stays bounded, app stays alive) — purely marker visibility/UX.

## Fix direction
1. Move `mark_truncated` to AFTER `strip_markup_to_text` in `docx_text`/`odt_text`/`xlsx_text` (match the
   pptx/epub idiom), so the marker survives stripping.
2. Also fix the exactly-`MAX_DECOMPRESSED_PART_BYTES` off-by-one flag (`buf.len() >= cap` flags a genuine
   entry of exactly 8MiB as truncated — use a read-one-more-byte probe or `>` semantics).
3. Optional: reconcile the content_text 4MiB outer cap vs the 8MiB inner cap so the search path preserves a
   visible truncation cue.

## Notes
Serialize AFTER CPE-1446 merges (same `doc_text.rs`/`content_text.rs`). Low priority — cosmetic on
already-degraded bomb docs. Epic CPE-724 (code-intelligence preview) / structured previews.
