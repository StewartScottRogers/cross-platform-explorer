---
id: CPE-1734
title: macro Convert writes through a link at the new name, then trashes the original
type: bug
priority: Medium
status: Doing
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

- [x] Decide whether a macro Convert may write through a link at `to` at all (expected: no) and record the
      decision at the site. — Decided: no. Recorded in `macro_convert_in_place`'s doc comment.
- [x] Add `cpe_server::fsutil::create_slot_refusal` (and/or `create_exclusive`) so a live **and** dangling
      link at `to` is refused before any byte is written, with the write-through wording, not the
      rename-destroys wording. — Both wired in, in that order (probe, then atomic open).
- [x] Decide the plain-clobber question explicitly: does a Convert overwrite an existing `photo.jpg`? —
      Decided: no, matching the Batch-Media engine's existing refusal of an unconfirmed overwrite.
      `create_slot_refusal`'s occupancy half now covers this for free.
- [x] The original must not be trashed on any path where the write was refused — assert on the
      **filesystem** (the source still there, the link still a link, the link's target not created), never
      on the returned `Result`. — All three new tests assert this way.
- [x] Test with `cpe_server::fsutil::make_dangling_link` (junction fallback, so it asserts for real on
      every runner) and `require_staged` per CPE-1717. — Both used.

## Work Log

**2026-08-25** — Fixed and closed.

**Root cause.** `macro_convert_in_place` was a bare `fs::write(to, converted)` with no slot guard, even
though `to` is a name being **claimed** (an extension swap; `from != to` is enforced above it), not a
file being edited. A link at `to` — live or dangling — was written through: the bytes landed at the
link's target (a path the user never named), `fs::write` reported `Ok`, and `trash::delete(from)` then
ran unconditionally because it sat behind that `Ok`'s `?`. A plain pre-existing `photo.jpg` was also
silently clobbered, with no confirmation.

**Harm reproduced on today's (pre-fix) code**, by temporarily reverting the guard while keeping the new
tests, then running them (`cargo test --lib cpe_1734_convert_refuses_a_dangling_link -- --nocapture`):

```
[EVIDENCE CPE-1734] result=Ok(()) target_conjured=true
  target_bytes=Some([255, 216, 255, 224, 0, 16, 74, 70, 73, 70, ... <valid JPEG> ..., 255, 217])
  original_still_here=false link_still_a_link=true
thread '...cpe_1734_convert_refuses_a_dangling_link_at_to_and_never_reaches_the_trash_step' panicked:
the link's target (...\photo.jpg-target-that-does-not-exist) must not have been conjured — pre-fix
`fs::write` followed the dangling link (`try_exists` reads it as free) and created it while reporting
success (result was Ok(()))
```

`result=Ok(())`, the link's target now holds the re-encoded JPEG bytes, the link itself is untouched, and
`original_still_here=false` — the original was trashed even though the write landed at the wrong file.
The live-link and plain-clobber legs reproduced the same shape (converted bytes landing in the victim
file / the pre-existing `photo.jpg`, both under `Ok(())`).

**Fix** (`src-tauri/src/lib.rs`, `macro_convert_in_place`): guard `to` with
`cpe_server::fsutil::create_slot_refusal` **before** `from` is even read — refusing a live link, a
dangling link, and now a plain occupied name too, with the write-through wording
(`"...writes THROUGH it..."`), never the rename-destroys wording. The write itself moved from
`fs::write` to `cpe_server::fsutil::create_exclusive` (`O_CREAT|O_EXCL`), which is the atomic half behind
the probe and closes the TOCTOU gap between the check and the open. `trash::delete(from)` needed no new
conditional: it already sat behind the write's `?`, so making the write-through case a genuine `Err`
(instead of a misleading `Ok`) is what makes "only trash the original once the bytes are really at `to`"
fall out of the existing `?` chain.

**Tests added** (`src-tauri/src/lib.rs`, three new `#[test]` fns near the existing CPE-1194 macro-convert
test): dangling link (runs on every runner via `make_dangling_link`'s junction fallback), live link
(CPE-1717 loud-skip on an unprivileged Windows account, `require_staged`), and a plain pre-existing file.
All assert on the filesystem — target not conjured / victim bytes untouched / occupied file untouched,
original still present with its exact original bytes, link still a link — never on the `Result` alone.
`from` uses a real, decodable PNG (`cpe_1734_test_png_bytes`, mirroring CPE-1194's
`cpe_1194_test_png_bytes`) because the pre-fix code path still reaches `fs::read` + `apply_ops` before
any write; an undecodable placeholder would have failed there with "unrecognized image format" instead
of demonstrating the actual defect.

**Verification.** `cargo clippy --all-targets -- -D warnings` clean in `src-tauri`, both default and
`--features sidecar-platform`. `cargo test` from `crates/server`: 2383 passed, 0 failed, 8 ignored.
`cargo test --lib` from `src-tauri`: 217 passed, 0 failed (214 pre-existing + 3 new).

**Known limits, stated rather than overclaimed:** this guard, like CPE-1857/1879's, covers the **final
path component only** — an intermediate directory junction inside the folder still lets a write land
outside the intended root; that is a separate, already-filed class. `create_exclusive` uses ordinary
`O_CREAT|O_EXCL`, not the handle-level `links`/reparse-point read `copy_file_onto_no_follow` uses, so
this is the CPE-1718 create-site primitive, not the CPE-1857 hard-link-count primitive — appropriate
here because `to` is a name being claimed, never an existing inode being overwritten, so there is no
hard-link-count question to ask at this site.

**Correction (Foreman, on merge — Security Auditor finding).** This Work Log originally went on to say
there was "no alternate-data-stream / Mark-of-the-Web concern either, for the same reason: nothing is
being copied onto an existing file." That answers the wrong question and is false. The ADS question at
a *create* site is not whether an existing stream gets clobbered — it is whether MotW is **carried**
from the source to the derived file. It is not. Measured:

```
[SEC-1025 MOTW] original's Zone.Identifier before =
  Some("[ZoneTransfer]
ZoneId=3
HostUrl=https://example.invalid/photo.png
")
[SEC-1025 MOTW] convert result = Ok(())
  converted file's Zone.Identifier after = None
  original still at its old name = false
```

So a macro Convert takes an internet-downloaded, MotW-tagged file, produces an **untagged** derivative,
and then trashes the tagged original — leaving the untagged copy as the only one. This is **CPE-1890**'s
class landing on a path that additionally destroys the evidence. It is **not a regression** (the
pre-fix bare `fs::write` did the same, and `stage_and_replace` carries attachments precisely because
CPE-1739 decided this matters), and the practical exposure is low because the output is a re-encoded
raster image. Recorded here as a stated limit rather than a denial, the same way the
intermediate-junction limit (CPE-1889) is.

## Notes

Filed by CPE-1725, 2026-08-14. Related: **CPE-1718** (the create-slot refusal this wanted, now wired in),
**CPE-1716** (the edit-site counterpart), **CPE-1194** (the trash-then-restore behaviour that makes the
source deletion recoverable — confirmed on the refused paths the source is never trashed at all, so
nothing needs restoring; CPE-1194's own round-trip test above this one in `lib.rs` still covers the
Recycle-Bin-and-back path when the write actually lands at `to`).
