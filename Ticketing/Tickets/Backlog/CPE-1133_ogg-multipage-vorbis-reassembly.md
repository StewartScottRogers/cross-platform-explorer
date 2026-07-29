---
id: CPE-1133
title: "OGG metadata reader: reassemble the Vorbis-comment packet across pages (read-side correctness)"
type: bug
component: Backend
priority: medium
status: Backlog
tags: ready
created: 2026-07-29
epic: CPE-1002
---

## Summary
`read_ogg` in `crates/server/src/media_meta_read.rs` (≈ line 241) reads OGG Vorbis-comment tags by naively
scanning the raw byte stream for the 7-byte packet signature `\x03vorbis` and handing everything *after* it
straight to `parse_vorbis_comment`. The function's own doc comment admits the gap:

> "Full multi-page packet reassembly is a later refinement."

An OGG *logical bitstream* is a sequence of **pages**, each with a 27-byte header (`OggS`, version, header
type, granule position, serial, sequence, checksum, `page_segments`) followed by a **segment table** and then
the segment (packet-fragment) bodies. A packet larger than one page's payload is split across consecutive
pages; the reader must strip each page's 27-byte header + segment table and concatenate only the segment
bodies to reconstruct the packet.

The current code does none of that. When the comment-header packet spans a page boundary (common with cover
art, many tags, or long comments), the raw scan leaves the **next page's 27-byte header + segment table
embedded in the middle of the comment data**. `parse_vorbis_comment` then reads a corrupt vendor length /
comment-count / entry lengths from those interspersed bytes → wrong or missing tags, and in the worst case a
huge bogus length that the parser must (and does, per the audit) defensively cap. This is a **real read-side
correctness bug**, not cosmetic.

## Design (headless, pure logic — no GUI, no new deps)
- Add a small OGG page walker to `media_meta_read.rs` (local helper; no external crate). Iterate pages from the
  `OggS` magic: for each page, parse the 27-byte header, read `page_segments` (byte 26), read that many
  segment-length bytes from the segment table, then take exactly `sum(segment lengths)` bytes of body.
- Reassemble packets by concatenating segment bodies: a segment whose lace value is `255` continues into the
  next segment/page; a lace value `< 255` (including `0`) **terminates** the current packet. Collect the first
  complete packet that starts with `\x03vorbis` (the Vorbis comment header) and hand its body **after** the
  7-byte signature to the existing `parse_vorbis_comment` (unchanged).
- Preserve existing behaviour on the happy path (single-page comment header must still parse identically) and
  on malformed/truncated input: **return empty `Vec`, never panic** — match the crate's skip-on-error
  convention. Guard every slice against out-of-bounds (truncated page header, segment table, or body).
- Keep it defensive against adversarial input: cap total bytes walked to the input length, and stop after the
  comment header packet is found (don't walk the whole audio stream).

## Acceptance Criteria
- [ ] A synthetic OGG whose Vorbis-comment packet is **split across two pages** parses all tags correctly
      (title/artist/etc.), matching what the same comment block yields when contained in a single page.
- [ ] The existing single-page OGG path still parses identically (no regression).
- [ ] Truncated / malformed OGG input (short page header, bad segment table, body running past EOF, missing
      comment header) returns an empty `Vec` and never panics — covered by tests.
- [ ] New unit tests build a multi-page OGG fixture in-code (extend the existing `build_vorbis` test helper into
      a page-framing helper) and assert reassembly; all `crates/server` tests pass.
- [ ] `cargo clippy --all-targets -D warnings` clean (both feature modes as CI runs them).

## Notes
- Queued in `.claude/workshift-metrics/CHECKPOINT.md` as genuine honest-headless work ("a legit read-side
  correctness slice", per `[[cpe-server-logic-audited]]` which flagged the str-slice-at-byte-offset and
  dead-truncation-notice patterns to watch).
- This read-side reassembler is also the safety net that would later unblock the risky OGG **write-back** path.
- Reference: Xiph OGG bitstream / page structure (Vorbis I spec §4, Ogg encapsulation RFC 3533).
