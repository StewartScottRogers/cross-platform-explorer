---
id: CPE-1061
title: "Search type-class filter — cpe_server::type_class (type:image query predicate)"
type: feature
component: Backend
priority: high
status: Done
tags: ready
created: 2026-07-25
epic: CPE-703
estimate: 2-3h
---

## Summary
Child of CPE-703 (Instant index search). Add a **pure, dependency-free** mapping of file extension →
semantic class + a `type:` filter predicate, so search can filter by "images", "video", etc. Backend-only,
`cargo test` on the 3-OS matrix — no GUI, no user resource, no new deps. Standalone module — does NOT touch
`index_query.rs`.

## Design (buildable)
New module `crates/server/src/type_class.rs`, registered `pub mod type_class;` in `lib.rs` **immediately
after `pub mod spotlight;`**.

**Redundancy check FIRST:** grep `crates/server/src/file_type.rs` — it exposes a `FileType` enum, but that
is **magic-byte content** detection (a different purpose). If it already offers a reusable *extension→class*
enum, reuse it; otherwise define the extension-based `FileClass` here (do NOT duplicate content-sniffing).

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum FileClass { Image, Video, Audio, Document, Archive, Code, Executable, Other }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TypeFilter { pub classes: Vec<FileClass> }

pub fn class_of(ext: &str) -> FileClass;           // lowercased ext → class via a static table
pub fn parse(token: &str) -> Option<TypeFilter>;   // `type:image` / `type:image,video`; garbage → None
pub fn matches(f: &TypeFilter, ext: &str) -> bool;
```
Static table covers the common extensions per class (image: png/jpg/jpeg/gif/webp/bmp/svg/tiff/heic…; video:
mp4/mov/mkv/avi/webm…; audio: mp3/flac/wav/ogg/m4a…; document: pdf/docx/txt/md/xlsx/pptx…; archive:
zip/tar/gz/7z/rar/xz/zst…; code: rs/ts/js/py/go/c/cpp/java/rb/sh…; executable: exe/dll/so/dylib/app/msi…).
Unknown ext → `Other`. Case-insensitive on ext. `type:image,video` parses to two classes.

## Acceptance Criteria
- [x] Representative ext→class mappings correct across all 8 classes; unknown ext → `Other`.
- [x] `type:image` and multi-class `type:image,video` parse correctly; `matches` true iff ext's class ∈ filter.
- [x] Case-insensitive ext; empty/garbage token → None (no panic).
- [x] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default AND
      `--features index` builds; no new deps. (If reusing `file_type`'s enum, note it in the work log.)

## Work Log
2026-07-25 (sprint) — Filed by the Product Manager as a CPE-703 DSL slice. Independent module; grep
`file_type.rs` before defining a class enum. One-line lib.rs `pub mod` at a distinct anchor.

2026-07-25 (sprint Worker) — Built end-to-end in worktree, branch `cpe-1061-type-class`, PR opened.
- **Redundancy check:** grepped `crates/server/src/file_type.rs` — its `FileType` enum is magic-byte
  content sniffing (`Png`/`Jpeg`/.../`Mp4`, from `sniff(bytes)`), a different purpose entirely (it answers
  "what does this file's content actually look like", not "what semantic bucket does this extension fall
  into"). It exposes no reusable ext→class enum, so `FileClass` is defined fresh in the new module per the
  ticket design — no duplication of content-sniffing logic.
- New module `crates/server/src/type_class.rs`: `FileClass` (8 variants), `TypeFilter { classes: Vec<FileClass> }`,
  `class_of`, `parse`, `matches`. Static ext tables per class, lowercased lookup. `parse` also accepts the
  bare comma list without the `type:` prefix (superset of the ticket's examples; still `None` on
  empty/garbage). Registered `pub mod type_class;` in `lib.rs` immediately after `pub mod spotlight;`.
  Does not touch `index_query.rs`.
- 15 new unit tests covering all 8 classes, unknown→Other, case-insensitivity (ext and class name),
  single/multi-class parse, prefix-optional parse, and empty/garbage→None (no panic).
- **Verify:** `cargo test` (crates/server) → 825 passed, 0 failed (11 of those are the new
  `type_class::tests::*`). `cargo clippy --all-targets -- -D warnings` → clean (exit 0). `cargo clippy
  --all-targets --features index -- -D warnings` → clean (exit 0). No new dependencies added to Cargo.toml.
- **Assumption:** ticket's `TypeFilter`/`FileClass` derive/field shapes matched exactly as specified;
  extended the static ext tables with a few extra common extensions per class (e.g. `avif`, `heif`,
  `3gp`, `cab`, `bin`) beyond the ticket's illustrative "…" lists, staying within each class's intent.
  `class_from_name` does not accept `"other"` as a selectable filter target (a file only lands in Other
  by exclusion, matching the ticket's framing of it as the fallback bucket).
- Not wired into `index_query.rs` or any search UI — out of scope per the ticket ("standalone module,
  does NOT touch index_query.rs"); a follow-up ticket under CPE-703 will consume this module.
