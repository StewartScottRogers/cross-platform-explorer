---
id: CPE-1746
title: 7z extraction writes through a symlink in the destination, on the path the UI actually uses
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-14
closed:
---

## The defect, measured

Extracting a `.7z` into a folder that already contains a symlink at an entry's name **writes the archive's
bytes through the link**, into a file nobody named, and reports success:

```text
[7z STREAMED (extract_7z_stream -> sevenz_rust::default_entry_extract_fn)]
    outcome      = Ok(ArchiveReport { done: 2, failed: 0, cancelled: false, errors: [] })
    victim bytes = Some("ARCHIVED A")        <-- was "VICTIM ORIGINAL"
    slot is link = Ok(true)                  <-- the link survives; the file it pointed at is overwritten
```

Measured on Windows and (by the CPE-1733 UAT, via Docker `rust:1-slim`) on Linux.

**This is on the shipping path.** `extract_archive_streamed` routes `.7z` to `extract_7z_stream`
(`crates/server/src/archive.rs`, the `sevenz_rust::decompress_file_with_extract_fn` call), and
`extract_archive_streamed` is what `start_archive_extract` — the queued extract the UI uses — calls. It is
not a theoretical path or a legacy command.

It is the exact hazard CPE-1733 guarded at every site `archive.rs` writes itself (rows 6–16 of that
module's table): `Ok` returned, bytes destroyed. 7z is the one extractor where the write happens **inside**
`sevenz_rust`, so there was no create site to guard, and CPE-1733 scoped it out and recorded it. Its UAT
then pointed out — correctly — that a recorded absence on a live path needs a ticket, not a comment.

## Why this is separate from CPE-1744

CPE-1744 carries the same family's *other* gaps: the `entry_name_is_safe` / `is_safe_name` delta (NTFS
alternate data streams), the leaf-only link check being escaped through a symlinked intermediate directory,
`tar` silently destroying a link, the one-shot/streamed ZIP divergence, and row 17's wording. Those are all
either not-live, lower severity, or wording. **This one loses a user's data on the path the UI uses**, so
it is split out to be pickable on its own rather than waiting behind an exploratory ticket. Do this one
first if only one gets done.

## What to do

- [ ] The write is inside `sevenz_rust::default_entry_extract_fn`, so the guard cannot go at the write.
      Two candidate shapes — **measure before choosing**, do not reason about it:
      1. Replace `default_entry_extract_fn` with our own per-entry writer inside the existing
         `decompress_file_with_extract_fn` callback (the callback already receives `entry_dest`), which
         puts a real create site under our control and lets it reuse `fsutil::create_slot_link_verdict` +
         `archive::entry_slot_action` exactly as rows 15/16 do.
      2. A pre-extraction sweep of `dest` for links matching the archive's entry names.
      Option 1 looks strictly better (no TOCTOU window, per-entry, same skip semantics as ZIP) — confirm
      that `default_entry_extract_fn` has no behaviour we would lose by not calling it.
- [ ] Match the ZIP rows' semantics: a confirmed link **skips** the entry and records it in
      `ArchiveReport::errors`; an unreadable slot **aborts** (`EntrySlotAction`, CPE-1733 UAT finding 6).
      Do not invent a third policy.
- [ ] Break the guard **on its own**, make a **distinct** test red, paste the real output, restore with
      `git checkout --`. Assert on the **victim's bytes and `symlink_metadata`** before unwrapping the
      `Result` — the bug returns `Ok`, so a `Result`-first assertion is unreachable when it regresses.
- [ ] Pin a **distinctive refusal substring**, not `is_err()`: on an unprivileged Windows runner a dangling
      junction makes `File::create` fail by itself (`Access is denied`, os error 5, measured for CPE-1733),
      so an `is_err()`-only leg passes through a deleted guard. The CPE-1733 UAT proved that pin
      load-bearing by construction.
- [ ] Cover **both** link kinds — dangling (`fsutil::make_dangling_link`) and live (`require_staged`,
      CPE-1717) — they are different measured behaviours and a guard can handle one without the other.
- [ ] Update `archive.rs`'s section-comment table (the "three extractors that are NOT our write loop"
      block) and `src/docs/explorer-archives.md`, both of which currently state this gap as open.

## Notes

Filed by the CPE-1733 worker from that ticket's UAT (finding F3), 2026-08-14, at the UAT's explicit
request that this one get its own ticket. Related: **CPE-1733** (the enumeration, the link guards, and
`entry_slot_action`), **CPE-1744** (the rest of the family), **CPE-628** (`entry_name_is_safe`, already
applied to 7z entry names for traversal), **CPE-1719** (write-through measured).
