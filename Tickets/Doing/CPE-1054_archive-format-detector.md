---
id: CPE-1054
title: "Archive format detector — cpe_server::archive_format (magic-byte sniff)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-705
estimate: 2-3h
---

## Summary
Child of CPE-705 (Archive & compression suite). Add a **pure, dependency-free** archive container detector so
the app can route a file to the right reader/extractor before opening it. Backend-only, `cargo test` on the
3-OS matrix — no GUI, no user resource, **no new deps** (byte-prefix + extension logic; does not call any
archive crate).

## Design (buildable)
New module `crates/server/src/archive_format.rs`, registered with `pub mod archive_format;` in
`crates/server/src/lib.rs` **immediately after the line `pub mod archive;`**.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat { Zip, Tar, Gz, TarGz, SevenZ, Iso, Unknown }

pub fn detect_format(bytes: &str_or_slice, name: &str) -> ArchiveFormat  // signature: (bytes: &[u8], name: &str)
```
Magic-byte sniff (check length before indexing — never panic on short/empty input):
- `PK\x03\x04` (or `PK\x05\x06` empty / `PK\x07\x08` spanned) at 0 → `Zip`
- `1F 8B` at 0 → gzip; disambiguate with `name`: `.tar.gz`/`.tgz` → `TarGz`, else `Gz`
- `7z\xBC\xAF\x27\x1C` at 0 → `SevenZ`
- `ustar` at offset 257 → `Tar`
- `CD001` at offset 0x8001 (32769) → `Iso`
- otherwise fall back to extension (`.zip`→Zip, `.tar`→Tar, `.tgz`/`.tar.gz`→TarGz, `.gz`→Gz, `.7z`→SevenZ,
  `.iso`→Iso); no match → `Unknown`.

Mirror the derive-stack + doc-comment conventions of `code_outline.rs`/`archive.rs`. This is distinct from
`file_type::detect_type` (general type detection) — it's archive-container disambiguation (esp. the
tar-in-gz nuance). O(1)/O(len) with no allocation beyond lowercasing the name.

## Acceptance Criteria
- [ ] Each signature maps to the right `ArchiveFormat`; `.tgz`/`.tar.gz` → `TarGz` while bare `.gz` → `Gz`.
- [ ] Truncated / empty / too-short byte inputs → no panic, fall to extension or `Unknown`.
- [ ] Extension-only fallback works when magic bytes are absent/ambiguous.
- [ ] `cargo test -p cpe-server` (from `crates/server`) green; clippy `--all-targets -- -D warnings` clean in
      default AND `--features index` builds; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a clean headless CPE-705 slice (planner/detector
layer over already-vendored formats; no new deps). Independent module; one-line lib.rs `pub mod` only.
