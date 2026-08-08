---
id: CPE-1446
title: "Office/ebook text preview: uncapped zip-entry decompression → deflate-bomb OOM (preview + content search)"
type: Bug
status: Backlog
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
