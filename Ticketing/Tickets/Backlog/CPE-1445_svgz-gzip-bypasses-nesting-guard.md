---
id: CPE-1445
title: "SVGZ (gzip) .svg bypasses the raw-byte nesting guard → uncatchable stack overflow + uncapped decompression OOM"
type: Bug
status: Backlog
priority: High
component: Backend
tags: [ready, security]
epic: CPE-718
created: 2026-08-07
---
## Vector (found in the CPE-1437 resource-exhaustion sweep, 2026-08-07)
A file named `*.svg` whose bytes begin with the gzip magic `1F 8B`. Prod path: browsing a folder →
thumbnail grid → `thumb_source.rs:~100` `rasterize_svg(&bytes, edge)` → `thumb_svg.rs:~173`
`xml_nesting_too_deep(bytes, …)` scans the **compressed** bytes, sees no `<` tags, returns `false`
(guard bypassed) → `thumb_svg.rs:~178` `usvg::Tree::from_data` detects the gzip magic
(`usvg parser/mod.rs:~98`) and calls `decompress_svgz` (`~:129`, a `read_to_end` with **no cap**), then
hands deeply-nested XML to `roxmltree` which recurses per nesting level.

## Concrete pathological inputs
- **Stack overflow:** a ~2–50 KB gzip of `<svg…>` + `"<g>".repeat(100_000)` + a rect + closes.
  Decompresses to a few MB of XML nested 100k deep → roxmltree blows any thread stack (the 2 MiB
  `spawn_blocking` stack included — gzip removes the file-size ceiling on depth that made raw SVG
  survivable). Uncatchable → whole process crashes.
- **OOM:** `"A".repeat(4GB)` gzipped to ~4 MB → `decompress_svgz` allocates ~4 GB.
Both stay under the 128 MiB file gate because the source file is tiny.

## Fix direction
In `rasterize_svg`, detect the gzip magic and decompress **with a bounded `.take(cap)`** BEFORE running
`xml_nesting_too_deep`, running the existing depth cap on the **decompressed** bytes; reject anything over
the cap (graceful Err). Closes both the nesting-guard bypass and the decompression bomb in one place.
This is distinct from CPE-1437 (clip/mask chains) and cheaper than the durable isolation in CPE-1444, but
**must serialize behind CPE-1437** — both edit `thumb_svg.rs`. Add gzipped-deep-nested + gzip-bomb fixtures
to the `thumb_svg` tests and the small-stack panic-safety probe.

## Effort / blast radius
S / tiny — one function, additive guard.
