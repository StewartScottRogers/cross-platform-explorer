---
title: "How to add MP4/MOV metadata write-back (write_mp4, CPE-1309/CPE-725) without corrupting video?"
date: 2026-08-04
tags: [write-mp4, mp4, mov, iso-bmff, moov, udta, ilst, stco, co64, media-metadata, cpe-725, cpe-1309, atom-rewrite, corruption-risk]
status: current
---

## Question
How to add MP4/MOV video metadata WRITE-BACK to the Media Metadata Studio, headlessly + safely (bad atom
surgery corrupts a user's video)?

## Finding (vetted against the code, 2026-08-04)

**Target model:** `moov▸udta▸meta▸ilst` iTunes-style atoms — exactly what the existing reader
`video_meta_read.rs::read_mp4` surfaces (9 keys: `©nam/©ART/©alb/©day/©cmt/©gen/©wrt/©too/cprt` →
Title/Artist/Album/Year/Comment/Genre/Composer/Encoder/Copyright, group `"video"`). Write MUST round-trip
through that reader — don't invent a 2nd target (bare QuickTime udta) it can't see.

**Companion change (easy to miss):** `read_mp4` marks all 9 keys `editable: false` today, and
`media_meta_edit.rs::apply_edits` rejects edits to read-only fields (`"video.Title is read-only"`). Flip the
9 to `editable: true` or every MP4 edit is rejected even once write_mp4 exists.

**THE safe strategy (never move mdat, never touch stco/co64):** `stco`/`co64` hold ABSOLUTE file offsets into
`mdat`; growing/moving anything before `mdat` silently invalidates them — file still parses, tag still reads
back, but playback desyncs (silent corruption a naive write→read→assert-tag test misses). Instead, mirror
`write_pdf`'s incremental-append precedent:
1. Byte-copy the whole original `moov` (every trak/stbl/stco/co64 carried through UNPARSED).
2. In the copy only, insert-or-replace the `udta▸meta▸ilst` atoms (synthesize udta/meta/ilst if absent, like
   `write_wav` does for LIST/INFO); recompute sizes bottom-up ilst→meta→udta→moov; leave all siblings
   (mvhd, every trak) byte-for-byte.
3. APPEND the modified moov at true EOF.
4. Shadow-disable the original moov by overwriting ONLY its 4-byte type `"moov"`→`"free"` in place (size
   field untouched → still self-delimiting dead space). This is the ONLY mutation to pre-existing bytes.
`mdat` never moves, so the verbatim-copied stco/co64 offsets stay correct. Layout-agnostic (works for both
faststart moov-before-mdat and trailing moov-after-mdat) — sidesteps offset fixup entirely.

**Refuse (honest Err, like write_ogg/write_pdf do):** fragmented MP4 (top-level `moof`/`mfra` — different
sample-location model) and a top-level `size==0` box that can't be safely made explicit before appending.

**Deps:** NONE. No MP4 crate vendored; box-walk primitives already exist (hand-rolled) in
`video_meta_read.rs` + `video_column.rs`. A mux crate (mp4/mp4parse) would force re-serializing the whole
moov (higher corruption risk — drops boxes it doesn't round-trip). **In-scope refactor:** factor the
duplicated `BoxHeader`/`read_box_header`/`find_child_box` into one shared internal `iso_bmff.rs` (writer is a
3rd consumer needing byte-range-copy + size-patch-in-place).

**Test gate (headless, cargo test, synthetic in-test fixtures like read_mp4's make_box):** build minimal MP4s
in BOTH layouts (moov-before-mdat + moov-after-mdat), each with a real trak/stbl/stco pointing at a known
`mdat` marker. Assert: tag round-trips via read_mp4; untouched tag survives; **mdat marker at its exact
original offset (load-bearing)**; **re-derive stco/co64 offsets from the REWRITTEN file and confirm they
still dereference to the correct mdat bytes (the actual guard for silent offset corruption — assertion #4)**;
old moov type now `"free"`; out still parses end-to-end; truncated input never panics; fragmented + size==0
each return Err.

**Biggest corruption risk to guard:** silently invalidating stco/co64 absolute offsets — a naive
write→read→assert-tag test passes while playback is corrupted. Mandatory guard = the offset-deref assertion.

**Scope:** one ticket (CPE-1309), v1 = non-fragmented, single mdat, either layout; refuse the rest. Deferred
(separate ticket): a compact/remux pass to reclaim accumulated `free` dead-space from repeated edits
(same file-growth-by-design tradeoff as write_pdf incremental updates).
