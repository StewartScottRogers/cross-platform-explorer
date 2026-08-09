---
id: CPE-1314
title: Wire write_wav/write_pdf/write_iptc/write_xmp/write_mp4 into the parser panic-safety harness
type: test
component: Backend
priority: medium
tags: ready
created: 2026-08-04
epic: CPE-1002
estimate: S
---

## Summary
Found by the shift-3 bench researcher. `crates/server/tests/parser_panic_safety.rs` currently exercises only
`write_exif`, `write_flac`, `write_id3v2`, `write_ogg`, `write_vorbis_comment` against the malformed-input
battery. The write codecs added since — `write_wav` (CPE-1298), `write_pdf` (CPE-1301), `write_iptc`/
`write_xmp` (CPE-1305), and `write_mp4` (CPE-1309) — are `pub fn` in the same modules but NOT wired into the
harness, so they have no adversarial-input coverage. (CPE-1297's "close panic gap" only added the read side.)
Now that #608 extracted the shared battery+harness into `tests/common/mod.rs`, close the write-side gap.

## Acceptance Criteria
- [ ] Add `write_wav`, `write_pdf`, `write_iptc`, `write_xmp`, `write_mp4` to the write-codec section of
      `parser_panic_safety.rs`, feeding each the malformed-input battery as its ORIGINAL container bytes (with
      a small representative set of edit fields), asserting NONE panic (Ok/Err only, never unwind) — mirror the
      exact pattern of the 5 existing write-codec entries and reuse `common::{run_battery, assert_no_panic}`.
- [ ] If any codec DOES panic on a malformed original, FIX it at the source (guard/boundary) and note it — the
      harness then guards the fix.
- [ ] `cargo test -p cpe-server --test parser_panic_safety` green; clippy clean (3 cpe-server CI modes). No new deps.

## Work Log
2026-08-04 (sprint) — Filed by the Foreman from the shift-3 bench researcher (grep-verified: 5 write codecs
absent from the harness). Sequenced after #608's tests/common refactor merged. Dispatched to a worker.
