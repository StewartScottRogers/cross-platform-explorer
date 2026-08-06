---
id: CPE-1347
title: "RAR entry-listing backend (cpe-server): pure-Rust RAR4/RAR5 header walk, ZERO new deps (no UnRAR)"
type: Feature
status: Done
priority: Low
component: cpe-server
tags: [ready]
epic: CPE-111
created: 2026-08-05
closed: 2026-08-05
---

## Goal

The **backend** half of RAR preview support (epic CPE-111), scoped as the ticket says: **list entries
read-only** (names + sizes) for `.rar`. NO decompression → NO non-free UnRAR library, NO new dep.

## Approach (vetted — see Library `gated-format-readers-dicom-raw-rar-2026-08-05`)

Reject the native `unrar` crate (non-free C) and the thinly-maintained pure-Rust `rar` crate (pulls
aes/cbc/hmac/pbkdf2 for full extraction we don't need). **Hand-roll a listing-only header walk** in a new
module `crates/server/src/rar.rs`, mirroring the style of `archive.rs`'s existing zip/tar/7z/iso dispatch.
Return the same `ArchiveEntry` shape those use (check `archive.rs` for the exact struct — name, size, is_dir).

- **RAR4**: marker `52 61 72 21 1A 07 00` (`Rar!\x1a\x07\x00`) → archive header → blocks. Each block has the
  common header (crc16, type u8, flags u16, head_size u16). File-header blocks (type `0x74`) carry
  pack_size/unp_size (+ high halves when the flag is set), name_size, and the inline name. Advance to the next
  block by `head_size` (+ `pack_size` for blocks with a data payload). Record name + unpacked size + dir flag.
- **RAR5**: marker `52 61 72 21 1A 07 01 00` (`Rar!\x1a\x07\x01\x00`) → **vint**-encoded (variable-length
  integer) headers. File-header records (header type `2`) carry flags, unpacked size, attributes, and a
  UTF-8 name. Walk by reading each header's size vint and skipping its data size. Implement a small
  `read_vint` helper.
- Public surface: `rar_entries(bytes_or_path) -> Result<Vec<ArchiveEntry>, String>`. Detect RAR4 vs RAR5 by
  marker. Unknown/corrupt/truncated → `Err`, never panic; cap entry count + validate all offsets/lengths
  in-bounds so a malformed archive can't loop or over-read.

## Acceptance criteria

- `rar_entries` lists names + sizes from both a synthetic **RAR4** and a synthetic **RAR5** fixture (build
  minimal valid byte blobs in tests — a couple of file entries + a directory entry). RAR5 vint parsing tested
  directly.
- Truncated / non-RAR / cyclic input → `Err`, never panics or hangs (add to panic-safety expectations).
- **Zero new dependencies** (`Cargo.toml` unchanged). `cargo test` + `cargo clippy --all-targets -- -D warnings`
  (both feature modes) green.

## Notes

Backend-only (no command / archive.rs dispatch wiring — that's the follow-up integration ticket, to keep this
a standalone non-colliding module PR). Parallelizable with CPE-1345/1346. If a real-world RAR5 vint edge case
is ambiguous, implement the common case + return `Err` for the exotic one (log the assumption).

## Work Log
- 2026-08-05 (workshift): RAR4/RAR5 listing (zero deps, no UnRAR). PR #640 squash-merged to main (25e25ef0). Worker(sonnet); independent Reviewer(+security lens) APPROVE + UAT PASS. Backend-only (no command/frontend wiring — follow-up). main compiles clean both feature modes.
