---
id: CPE-1448
title: "Doc-preview '(truncated)' marker can be silently swallowed (docx mid-tag strip; content-search 4MiB outer cap)"
type: Bug
status: Done
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

## Work Log (2026-08-07)

All three fixes landed in `crates/server/src/doc_text.rs` and `crates/server/src/content_text.rs`:

1. **Mid-tag strip swallowing the marker** — `zip_read_text`/`zip_read_text_optional` no longer call
   `mark_truncated` themselves; they now return `(raw_text, truncated)` and the truncated flag is
   threaded through `docx_text`/`odt_text`/`xlsx_text`, which call `mark_truncated` AFTER
   `strip_markup_to_text` — matching the idiom `pptx_text`/`epub_text` already used. The marker can no
   longer land inside an unclosed tag's "in_tag" run and get stripped away with it.
2. **Exact-cap off-by-one** — `read_entry_capped` now reads `cap + 1` bytes (`take(MAX_DECOMPRESSED_PART_BYTES
   + 1)`) instead of exactly `cap`, flags `truncated` only when it actually got more than `cap` bytes
   (`buf.len() > cap`, not `>=`), and truncates the buffer back to `cap` before returning. An entry whose
   real size is exactly the cap now reads cleanly with no false "(truncated)" marker; memory stays bounded
   at `cap + 1` bytes (one byte of headroom, not unbounded).
3. **content-search 4MiB outer cap swallowing the cue** — `content_text.rs`'s `cap()` now reserves room
   for and re-appends its own `TRUNCATION_MARKER` ("… (truncated)") whenever IT is the one doing the
   cutting, regardless of whether the input already carried doc_text's own inner-cap marker. So a document
   truncated by the 8MiB inner cap that then gets trimmed further by the 4M-char outer cap still shows a
   visible truncation cue on the search/index path; text under the outer cap passes through unchanged (no
   marker added when nothing was actually cut).

### Test approach
Ran synchronously in `crates/server`:
- `cargo build` — clean.
- `cargo clippy --all-targets -- -D warnings` — clean, 0 warnings.
- `cargo clippy --all-targets --features specta -- -D warnings` — clean, 0 warnings.
- `cargo test` — 1721 unit tests + all integration test binaries pass, 0 failures.

New/adjusted tests:
- `doc_text::tests::docx_text_shows_truncation_marker_even_when_the_cap_lands_mid_tag` — a docx whose
  `word/document.xml` has an unclosed `<w:t attr="...">` tag straddling the 8MiB cap boundary; asserts the
  marker survives stripping and content past the cut never appears. Fails against the pre-fix ordering
  (marker got eaten), passes now.
- `doc_text::tests::docx_text_does_not_falsely_mark_an_exactly_cap_sized_entry_as_truncated` — a
  `word/document.xml` entry built to be exactly `MAX_DECOMPRESSED_PART_BYTES` bytes; asserts no
  "(truncated)" marker appears.
- `content_text::tests::content_search_path_preserves_a_truncation_cue_through_both_caps` — a docx whose
  `<w:t>` run is a 64MiB deflate bomb (so nearly all of the 8MiB doc_text reads survives stripping as plain
  text, well over the 4M-char outer cap); asserts the text handed to content search is still bounded by
  the outer cap AND still contains a truncation cue.
- `content_text::tests::size_cap_truncates_extracted_text` — strengthened to also assert the marker is
  present when `cap()` actually cuts.
- `content_text::tests::size_cap_leaves_short_text_untouched_with_no_marker` — new: text under the cap
  passes through byte-for-byte unchanged, no marker added.
