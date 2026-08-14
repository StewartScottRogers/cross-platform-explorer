---
id: CPE-1734
title: macro Convert writes through a link at the new name, then trashes the original
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-14
closed:
---

## Problem

Found while inventorying the `fs::write` siblings for **CPE-1725** (the dangling-link save-parity
decision). `macro_convert_in_place` (`src-tauri/src/lib.rs`) is a macro `Convert` step: it reads `from`,
re-encodes to the `detail` extension, writes the result at `to`, and then routes `from` to the OS trash.

The write is a bare `fs::write(to, converted)` with **no slot guard at all**:

```rust
fs::write(to, converted).map_err(|e| format!("could not write {to}: {e}"))?;
trash::delete(from).map_err(|e| format!("could not trash {from}: {e}"))?;
```

`to` is a name being **claimed** (`photo.png` → `photo.jpg`; `from != to` is enforced above), so the
guard it wants is CPE-1718's `create_slot_refusal` + `create_exclusive`, not CPE-1716's
`replace_file_contents` — resolving a link is wrong for a name being claimed, which is why CPE-1725 did
not simply route this through the same helper it routed `write_file_text` through.

Two concrete consequences, both of the shapes this family has already measured elsewhere:

1. **A link at `to` is written through.** `fs::write` follows the final component, so the converted bytes
   land at the link's target — a file the user never named, possibly outside the folder — and the link
   survives reporting success. A **dangling** link reads as a free name (`try_exists` follows links, so it
   answers `Ok(false)`) and the target is conjured. Measured for this exact `O_CREAT` shape on Windows by
   CPE-1718 (`File::create -> Ok`, 4096 bytes at the target, slot still a link) and on the CPE-1725 save
   path.
2. **The original is then trashed regardless.** Unlike a plain overwrite, this step deletes the source
   after the write, so a redirected write is not merely surprising — the file the user was looking at is
   gone from that folder and the replacement is somewhere else.

There is also **no clobber check**: a plain pre-existing file at `to` is silently overwritten. That is a
separate question (the Batch-Media engine refuses an unconfirmed in-place overwrite; this path does not)
and may or may not be desired for a macro, but it should be decided rather than inherited from
`fs::write`.

## Why it was not fixed in CPE-1725

CPE-1725's question was "what does a dangling link at a **whole-file save over a path the user opened**
mean", and it answered it for both save paths. This is a **create** site, so it needs the other guard, on
a command with macro-rollback semantics, and a real test needs image fixtures (`png_bytes`-style) that the
`src-tauri` test module does not currently have. Guessing at it inside a ticket about a different
primitive is how the two-guards-look-alike mistakes in this family happened.

## Acceptance criteria

- [ ] Decide whether a macro Convert may write through a link at `to` at all (expected: no) and record the
      decision at the site.
- [ ] Add `cpe_server::fsutil::create_slot_refusal` (and/or `create_exclusive`) so a live **and** dangling
      link at `to` is refused before any byte is written, with the write-through wording, not the
      rename-destroys wording.
- [ ] Decide the plain-clobber question explicitly: does a Convert overwrite an existing `photo.jpg`?
- [ ] The original must not be trashed on any path where the write was refused — assert on the
      **filesystem** (the source still there, the link still a link, the link's target not created), never
      on the returned `Result`.
- [ ] Test with `cpe_server::fsutil::make_dangling_link` (junction fallback, so it asserts for real on
      every runner) and `require_staged` per CPE-1717.

## Notes

Filed by CPE-1725, 2026-08-14. Related: **CPE-1718** (the create-slot refusal this wants), **CPE-1716**
(the edit-site counterpart), **CPE-1194** (the trash-then-restore behaviour that makes the source deletion
recoverable — worth confirming it still is when the write went to the wrong place).
