---
id: CPE-1309
title: MP4/MOV video metadata write-back (write_mp4) — Media Metadata Studio
type: feature
component: Backend
priority: medium
tags: ready
created: 2026-08-04
epic: CPE-725
estimate: 4h+
---

## Summary
Child of CPE-725. The Studio reads MP4/MOV video metadata (`video_meta_read.rs::read_mp4`, 9 iTunes-style
keys, group `"video"`) but can't write it (`is_writable("mp4")` is false; no `write_back` arm). Add
`write_mp4`. Full vetted plan: research library
`.claude/research-library/entries/mp4-metadata-writeback-plan-2026-08-04.md` — READ IT FIRST; it is the spec.

## The safe strategy (non-negotiable — anything else risks silent playback corruption)
`stco`/`co64` hold ABSOLUTE offsets into `mdat`. NEVER move `mdat`, NEVER touch `stco`/`co64`. Instead
(mirror `write_pdf`'s incremental append):
1. Byte-copy the whole original `moov` (all trak/stbl/stco carried through unparsed).
2. In the copy, insert-or-replace the `udta▸meta▸ilst` atoms (synthesize udta/meta/ilst if absent);
   recompute sizes bottom-up ilst→meta→udta→moov; every sibling byte-for-byte.
3. Append the modified `moov` at true EOF.
4. Shadow the original `moov` by overwriting ONLY its 4-byte type `"moov"`→`"free"` in place.

## Acceptance Criteria
- [ ] `write_mp4` writes the 9 keys to `moov/udta/meta/ilst`, round-tripping through `read_mp4`.
- [ ] Flip those 9 keys to `editable: true` in `read_mp4` (else `apply_edits` rejects every MP4 edit).
- [ ] `is_writable` + `write_back` dispatch extended for `mp4`/`mov`/`m4v` (+ `m4a` if the reader handles it).
- [ ] Refuse (clear `Err`, no guess): fragmented MP4 (top-level `moof`/`mfra`); a top-level `size==0` box that
      can't be safely made explicit before appending.
- [ ] In-scope refactor: factor the duplicated `BoxHeader`/`read_box_header`/`find_child_box` (in
      `video_meta_read.rs` + `video_column.rs`) into ONE shared internal `iso_bmff.rs`; all three consumers use it.
- [ ] Tests (synthetic in-test fixtures, BOTH moov-before-mdat and moov-after-mdat layouts): tag round-trips;
      untouched tag survives; **mdat marker at its exact original offset**; **re-derive stco/co64 offsets from
      the REWRITTEN file and confirm they deref to the correct mdat bytes (load-bearing — the guard against
      silent offset corruption)**; old moov type now `"free"`; output re-parses clean; truncated input never
      panics; fragmented + size==0 each return Err.
- [ ] No new deps (bounded hand-roll). `cargo test` + clippy (3 cpe-server modes) green. No specta struct
      change expected (video keys already exist); if any, regen bindings.

## Work Log
2026-08-04 (sprint) — Filed by the Foreman from the vetted research plan (Library entry above).
Dispatched to an opus worker after CPE-1308 merges (shares media_meta.rs).
