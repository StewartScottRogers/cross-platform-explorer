---
id: CPE-1061
title: "Search type-class filter — cpe_server::type_class (type:image query predicate)"
type: feature
component: Backend
priority: high
status: Doing
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
- [ ] Representative ext→class mappings correct across all 8 classes; unknown ext → `Other`.
- [ ] `type:image` and multi-class `type:image,video` parse correctly; `matches` true iff ext's class ∈ filter.
- [ ] Case-insensitive ext; empty/garbage token → None (no panic).
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default AND
      `--features index` builds; no new deps. (If reusing `file_type`'s enum, note it in the work log.)

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-703 DSL slice. Independent module; grep
`file_type.rs` before defining a class enum. One-line lib.rs `pub mod` at a distinct anchor.
