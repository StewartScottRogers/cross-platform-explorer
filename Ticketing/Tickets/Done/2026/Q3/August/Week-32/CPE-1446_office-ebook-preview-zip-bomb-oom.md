---
id: CPE-1446
title: "Office/ebook text preview: uncapped zip-entry decompression → deflate-bomb OOM (preview + content search)"
type: Bug
status: Done
priority: High
component: Backend
tags: [ready, security]
epic: CPE-724
created: 2026-08-07
---
## Vector (found in the CPE-1437 resource-exhaustion sweep, 2026-08-07)
`crates/server/src/doc_text.rs:~81` `zip_read_text` does `entry.read_to_string(&mut buf)` with **no
output cap**; same pattern in `zip_read_text_optional` (`~:101`), `pptx_text` (`~:145`), `epub_text`
(`~:190`). Prod paths that hit it automatically:
- Preview pane → `read_preview_info_impl` (`src-tauri/src/lib.rs:~885`) for docx/odt/epub.
- Content search → `content_text.rs:~81` for docx/xlsx/pptx (an automatic scan path).

## Concrete pathological input
A valid ~1 MB `.docx` (a zip) whose `word/document.xml` entry is a deflate stream of ~4 GB of
spaces/zeros (compresses ~1000:1). Selecting it → `docx_text` → `read_to_string` allocates ~4 GB → OOM.
The 128 MiB `ensure_previewable_size` gate is on the **compressed** file and does not help. (epub caps its
*output* at 128 KiB but only AFTER fully inflating each entry into `buf`, so one bomb entry OOMs first.)

## Fix direction
Replace each `read_to_string`/`read_to_end` on a zip entry with a capped read —
`entry.take(MAX_DECOMPRESSED_PART_BYTES).read_to_string(...)` (a few MiB is plenty for a text preview) —
and treat overflow as truncated/`Err`. Optionally wire the already-built (currently unused)
`archive_safety::expansion_ratio` scorer using the entry's `compressed_size()`/`size()`. Fixing the
`doc_text.rs` helper covers both the preview and content-search callers. Add a bomb-docx fixture test.

## Effort / blast radius
S / small — one helper + a couple of enumerate-loops in `doc_text.rs`; disjoint from the SVG (thumb_svg.rs)
and thumbnails_stream (lib.rs) work, so parallel-safe.

## Work Log
2026-08-07 (sprint) — Fixed. Added `MAX_DECOMPRESSED_PART_BYTES = 8 MiB` in `doc_text.rs` and a
`read_entry_capped` helper that wraps every zip-entry reader in `Read::take(MAX_DECOMPRESSED_PART_BYTES)`
before reading — the actual OOM defense, since it bounds the read regardless of what the entry's
`size()`/`compressed_size()` metadata claims (a crafted zip can lie about both, so a ratio pre-check
alone — e.g. `archive_safety::expansion_ratio` — is only ever a cheap early-warning, never a substitute).
8 MiB was chosen because a legitimate single office/ebook document *part* (`word/document.xml`, a
`sharedStrings.xml`, one PPTX slide, one EPUB content document) is typically well under 1 MiB of XML even
for a long real-world document — 8 MiB is generous headroom for genuine content while keeping a bomb
entry's cost bounded to a fixed, small allocation no matter how large it claims to inflate to (a real
~1000:1 deflate bomb going from a few KiB on disk to gigabytes decompressed is now cut off at 8 MiB
instead).

Reads into a `Vec<u8>` (`read_to_end`, not `read_to_string`) and lossy-decodes via
`String::from_utf8_lossy` — this sidesteps a cap landing mid multi-byte UTF-8 codepoint, which would
otherwise turn a truncated read into an `Err` instead of degrading gracefully. Overflow (cap hit) is
signalled by appending a "… (truncated)" marker to the returned text, matching `epub_text`'s pre-existing
whole-document truncation idiom — never a hard `Err` for that case, so a bomb entry degrades to
truncated/partial text for that one file rather than crashing or failing the whole preview/search walk
(matches the repo's skip-on-error discipline).

Every uncapped read site fixed, all four named in the ticket:
- `zip_read_text` (docx/odt callers) — now `read_entry_capped` + `mark_truncated`.
- `zip_read_text_optional` (xlsx caller) — same.
- `pptx_text` — capped per-slide, BEFORE `strip_markup_to_text` runs on each slide's XML.
- `epub_text` — capped per-entry INSIDE the loop, before accumulating into `out`. This was the subtle
  half of the bug: the existing 128 KiB *output* cap (`out.len() > 128 * 1024`) is only checked between
  content documents, so a single bomb `.xhtml`/`.html` entry would previously fully inflate via
  `read_to_string` before that check was ever reached again. Now `read_entry_capped` bounds that one
  entry's own read to 8 MiB regardless, and an `"… (entry truncated)"` marker is appended per-entry so a
  truncated document is visibly marked.

`content_text.rs` (the content-search caller for docx/xlsx/pptx) needed no separate fix — it dispatches
straight into these same `doc_text.rs` extractors and just applies its own downstream `MAX_EXTRACTED_CHARS`
*output* cap, so it inherits the per-entry read cap automatically. Confirmed no independent uncapped
`read_to_string`/`read_to_end` in that file.

Tests (`crates/server/src/doc_text.rs`, all passing): added
`docx_text_caps_a_deflate_bomb_entry_instead_of_inflating_it_fully` and
`epub_text_caps_a_deflate_bomb_content_document_before_accumulating` — each builds a real `.docx`/`.epub`
(via the `zip` crate, already a dep) whose one entry is a genuine deflate stream of 64 MiB of a repeated
byte (8x the cap, streamed into the zip writer in 1 MiB chunks so building the fixture itself never holds
the whole payload in memory), asserts the on-disk file compresses to well under 1 MiB (proving it's really
bomb-shaped), then asserts the extracted text is capped near `MAX_DECOMPRESSED_PART_BYTES` (not the full
64 MiB) and carries the truncation marker — this is the assertion that proves the cap actually stopped the
read rather than allocating the whole decompressed stream. Also strengthened the existing
`docx_text_extracts_paragraph_text` test with an explicit `!text.contains("(truncated)")` assertion, so a
normal small legitimate `.docx` is verified to preview its text correctly with no over-truncation.

Verification: `cargo build`, `cargo clippy --all-targets -- -D warnings` (default features) and
`cargo clippy --all-targets --features specta -- -D warnings` both clean; `cargo test` in `crates/server`
— 1701 passed, 0 failed (one `organize_apply` collision test flaked once under full parallel load,
reproduced as passing both in isolation and on a clean full rerun — confirmed pre-existing test flakiness
unrelated to this change, not a regression). No new dependencies; no `specta::Type` struct touched, so no
bindings regen needed.
