---
id: CPE-1308
title: Media-metadata write-back polish — EXIF clear-symmetry, IPTC UTF-8 charset, 8BIM survivor test
type: task
component: Backend
priority: medium
tags: ready
created: 2026-08-04
epic: CPE-725
estimate: 2-3h
---

## Summary
Three follow-ups surfaced by the CPE-1305 gauntlet (opus reviewer + re-verifier), all in
`crates/server/src/media_meta.rs` + `media_meta_write.rs`:

1. **EXIF clear-symmetry (re-verify finding #2).** IPTC/XMP now strip their segment when their last field is
   cleared, but EXIF doesn't: `write_jpeg` runs `write_exif` when `edited("exif")`, and `write_exif` returns
   `Err("no editable EXIF fields to write")` when the editable overrides are empty — so clearing the last
   editable EXIF field (e.g. the only ImageDescription) ERRORS the whole save instead of persisting the clear.
   Fix: clearing an editable EXIF field must persist (field gone on reopen) while preserving intrinsic EXIF
   (Make/Model/GPS/etc.) byte-for-byte; at minimum it must NOT error. Prefer real removal of the cleared
   editable field from the EXIF IFD; if full removal is impractical, skip gracefully (no error) — but a
   graceful-skip must be explicitly justified, not silent.

2. **IPTC UTF-8 CodedCharacterSet (opus reviewer note).** `write_iptc` omits the record-1 `1:90`
   CodedCharacterSet (ESC `% G`) declaration, so exiftool/Photoshop interpret non-ASCII IIM bytes as
   ISO-8859-1 (mojibake). Add the `1:90` ESC-%-G UTF-8 declaration so non-ASCII captions/keywords read back
   correctly in real tools. ASCII stays byte-identical.

3. **IPTC surviving-8BIM test (re-verify finding #3).** The path where the last IPTC field is cleared but a
   NON-IPTC 8BIM resource remains (APP13 rewritten to carry only survivors) is untested. Add a round-trip
   test: seed a JPEG with an IPTC caption AND another 8BIM resource, clear the caption, assert the APP13 is
   rewritten well-formed with the survivor present and the IPTC IRB gone.

## Acceptance Criteria
- [ ] EXIF: clearing the last editable EXIF field persists (round-trip: set → clear → reopen shows gone),
      intrinsic EXIF + image preserved; no error. Falsifiable test.
- [ ] IPTC `1:90` UTF-8 CodedCharacterSet emitted; a non-ASCII caption round-trips through read_iptc AND the
      declaration byte-sequence is present; ASCII output unchanged (assert a known ASCII fixture is byte-identical
      to pre-change where practical).
- [ ] Surviving-8BIM clear test added + green.
- [ ] `cargo test -p cpe-server` green; `clippy --all-targets -D warnings` clean in the 3 cpe-server CI modes.

## Work Log
2026-08-04 (workshift) — Filed by the Foreman from the CPE-1305 gauntlet findings. Dispatched to a worker.
