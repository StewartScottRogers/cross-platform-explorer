---
id: CPE-1311
title: Malformed-input panic-safety coverage for binary_preview/data_preview path parsers
type: test
component: Backend
priority: medium
tags: ready
created: 2026-08-04
estimate: 2-3h
---

## Summary
Found by the shift-3 bench researcher. `binary_preview.rs` (`pe_info`/`midi_info`/`wasm_info`/`torrent_info`
via goblin/midly/wasmprinter/serde_bencode) and `data_preview.rs` (`spreadsheet_info`/`sqlite_info` via
calamine/rusqlite) run third-party parsers on arbitrary user files but have NO adversarial-input coverage —
unlike every media_meta codec, which sits behind `parser_panic_safety.rs`. Only `hex_dump` + one trivial
`parquet_info` case are tested. These 6 entrypoints take a `path: &str` (not `&[u8]`), so they need a small
temp-file wrapper around the malformed-input battery generator rather than direct reuse.

## Acceptance Criteria
- [ ] New test (e.g. `crates/server/tests/binary_data_preview_panic_safety.rs`, or extend an existing panic
      test file) driving `pe_info`, `midi_info`, `wasm_info`, `torrent_info`, `spreadsheet_info`, `sqlite_info`
      with the malformed-input battery (truncated, zeroed, random, valid-header+garbage-body, empty) written to
      temp files, asserting NONE panic (each returns `Ok`/`Err`, never unwinds).
- [ ] Reuse the existing battery generator from `parser_panic_safety.rs` if practical (wrap it to write bytes
      to a NamedTempFile and pass the path); do not duplicate the generator.
- [ ] If any entrypoint DOES panic on a malformed input (goblin/midly are historically prone), FIX it
      (catch/guard) as part of this ticket and note it — the test then guards the fix.
- [ ] `cargo test -p cpe-server` green; clippy clean (3 cpe-server CI modes).

## Work Log
2026-08-04 (workshift) — Filed by the Foreman from the shift-3 bench researcher (grep-verified: 5 of these
entrypoints have zero adversarial coverage). Dispatched in parallel with CPE-1309.
