---
id: CPE-880
title: Fix RTF preview panic on a malformed \'-escape before a multi-byte char
type: bug
component: Server
priority: high
tags: ready
epic: CPE-706
created: 2026-07-21
closed: 2026-07-21
status: Done
---

## Summary
`doc_text::rtf_text` decoded RTF `\'XX` byte escapes with `u8::from_str_radix(&raw[i+1..i+3], 16)`, slicing
the source **`String` at byte offsets**. RTF normally follows `\'` with two ASCII hex digits, but a
malformed/adversarial `.rtf` where `\'` is immediately followed by a multi-byte UTF-8 character (e.g. `€`,
3 bytes) makes offset `i+3` land **mid-character**, so the `&str` slice **panics** ("byte index N is not a
char boundary"). Because the preview pane reads untrusted files, that's a crash vector on hostile input.

Fix: decode the two bytes directly via a small `hex_digit` helper — never slice the `str` — so a non-hex
byte (incl. any part of a multi-byte char) is simply skipped and parsing continues.

## Bug
- Before: opening a crafted `.rtf` (a `\'` followed by a 3- or 4-byte UTF-8 char) panicked the RTF text
  extractor.
- After: the bad escape is skipped, valid `\'41` still decodes to `A`, and extraction completes.

## Acceptance Criteria
- [x] `rtf_text` does not panic on `\'` followed by a multi-byte UTF-8 char.
- [x] Valid `\'XX` hex escapes still decode; parsing continues past a malformed one.
- [x] Full `cpe-server` suite (163) + `cargo clippy --all-targets -D warnings` green.

## Work Log
- 2026-07-21 (autonomous) — Found the char-boundary slice panic while auditing the hand-rolled parsers in
  the preview providers. Replaced the `raw[..]` slice with byte-wise `hex_digit` decoding; added a
  regression test that panics against the old code. 3/3 doc_text tests + full suite pass; clippy clean.
