---
id: CPE-1001
title: Magic-byte file-type detection + extension-mismatch flagging
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-1000
---

# CPE-1001 — Magic-byte file-type detection + extension-mismatch flagging

## Summary

The pure engine behind epic CPE-1000 ("True file-type detection & extension-mismatch flagging"): a
magic-byte signature sniffer over an in-memory byte slice, plus a mismatch check that flags a claimed
extension that disagrees with the sniffed type (e.g. a `.jpg` that is actually a Windows PE executable in
disguise). No filesystem I/O, no new dependencies — the caller supplies the bytes (a future ticket wires a
capped byte-prefix read + a UI warning on top of this).

New module `crates/server/src/file_type.rs`.

## Design

- `pub enum FileType` — Png, Jpeg, Gif, Bmp, WebP, Tiff, Pdf, Zip, Gzip, SevenZip, Rar, Elf, Pe, Wasm,
  Flac, Ogg, Mp3, Wav, Mp4. `Debug, Clone, Copy, PartialEq, Eq`.
  - `label(self) -> &'static str` — human-readable name (e.g. "PNG image").
  - `extensions(self) -> &'static [&'static str]` — canonical lowercased extensions (no dot).
- `pub fn detect_type(bytes: &[u8]) -> Option<FileType>` — bounds-checked magic-signature match; `None`
  for unknown/too-short input, never panics.
- `pub struct Mismatch { pub detected: FileType, pub actual_ext: String }` and
  `pub fn mismatch(bytes: &[u8], ext: &str) -> Option<Mismatch>` — lowercases `ext` (strips a leading
  dot), detects the type, and returns `Some` only when a type *was* detected and `ext` isn't among its
  `extensions()`. Unknown bytes → `None` (nothing to judge); matching extension → `None` (no complaint).
- `pub mod file_type;` added to `lib.rs` with a doc comment.
- Pure std, zero new dependencies.

## Acceptance Criteria

- [x] `detect_type` recognises: PNG, JPEG, GIF (87a/89a), BMP, TIFF (both byte orders), PDF, ZIP (local
  header + empty + spanned variants), GZIP, 7z, RAR, ELF, PE, WASM, FLAC, OGG, MP3 (ID3 tag or frame
  sync), WAV (RIFF/WAVE), WebP (RIFF/WEBP), MP4 (ftyp at offset 4).
- [x] Every byte access is bounds-checked; empty and 1-byte input return `None`, never panic.
- [x] `mismatch` treats ZIP-container formats (docx/xlsx/pptx/odt/ods/odp/epub/jar/apk) as matching ZIP
  bytes — no false-flag on Office/OpenDocument/ebook files.
- [x] `mismatch` handles a leading-dot extension and is case-insensitive.
- [x] `mismatch` returns `None` for unknown bytes regardless of claimed extension (nothing to judge).
- [x] Zero new dependencies; pure over bytes, no filesystem I/O.
- [x] `pub mod file_type;` declared in `lib.rs` with a doc comment.
- [x] `cargo test --lib file_type` passes (33 tests).
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `cargo clippy --all-targets --features index -- -D warnings` clean.

## Work Log

- 2026-07-24 — Built `file_type.rs` end-to-end: `FileType` enum (19 variants) with `label`/`extensions`,
  bounds-checked `detect_type`, and `mismatch`.
  - **Signature coverage:** PNG (`89 50 4E 47`), JPEG (`FF D8 FF`), GIF (`GIF87a`/`GIF89a`), BMP (`42 4D`),
    TIFF (`49 49 2A 00` / `4D 4D 00 2A`), PDF (`%PDF`), ZIP (`50 4B 03 04` local header, `50 4B 05 06`
    empty archive, `50 4B 07 08` spanned archive), GZIP (`1F 8B`), 7z (`37 7A BC AF 27 1C`), RAR
    (`52 61 72 21 1A 07`), ELF (`7F 45 4C 46`), PE (`4D 5A` — checked last since it's the shortest, most
    generic signature in the set, so a longer/more specific match always wins first), WASM
    (`00 61 73 6D`), FLAC (`fLaC`), OGG (`OggS`), MP3 (`ID3` tag *or* an 11-set-bit MPEG frame sync:
    `0xFF` followed by a byte with its top 3 bits set), WAV (`RIFF` at 0 + `WAVE` at offset 8), WebP
    (`RIFF` at 0 + `WEBP` at offset 8), MP4 (`ftyp` at offset 4, ISO base media box layout).
  - **Container-extension decision:** ZIP is a container format reused by Office Open XML, OpenDocument,
    EPUB, JAR, and APK. `FileType::Zip.extensions()` lists all of `zip/jar/apk/docx/xlsx/pptx/odt/ods/odp/
    epub` so `mismatch` never flags a `.docx` (which genuinely *is* ZIP bytes) as a "renamed .zip" — the
    container format is correct even though the payload inside is a different, higher-level format that
    this ticket doesn't attempt to distinguish (that's out of scope: this is magic-byte sniffing, not a
    full ZIP-entry inspector). Same reasoning applied more narrowly to JPEG (`jpg`/`jpeg`/`jpe`), TIFF
    (`tif`/`tiff`), OGG (`ogg`/`oga`/`ogv`), MP4 (`mp4`/`m4a`/`m4v`), GZIP (`gz`/`tgz`), and PE
    (`exe`/`dll`, same container format for both) — each lists every common extension alias so a correctly
    -named file of that family is never false-flagged.
  - **ELF extension:** ELF binaries are commonly extensionless on Linux; `extensions()` returns `["so"]`
    (the one common case with a canonical extension — shared libraries). A bare extensionless executable
    is outside `mismatch`'s scope (it takes a required `ext` argument; a future caller decides what to do
    when a file has no extension at all).
  - Verified (PowerShell, `crates/server`, `$env:PATH += ";$env:USERPROFILE\.cargo\bin"`):
    `cargo test --lib file_type` → 33/33 passed; `cargo clippy --all-targets -- -D warnings` clean;
    `cargo clippy --all-targets --features index -- -D warnings` clean. No clippy fixes were needed.
  - Scope note: epic CPE-1000 doesn't yet have a filed `Tickets/Epics/CPE-1000*.md` brief in this repo at
    the time of this ticket; per the work order this ticket only touches `file_type.rs` + the one `lib.rs`
    module line + this ticket file, so the epic file wasn't created here. Frontmatter still references
    `epic: CPE-1000` as instructed.
  - Status → Done; ACs checked; moving to
    `Tickets/Done/2026/Q3/July/Week-30/CPE-1001_magic-byte-file-type.md`.
