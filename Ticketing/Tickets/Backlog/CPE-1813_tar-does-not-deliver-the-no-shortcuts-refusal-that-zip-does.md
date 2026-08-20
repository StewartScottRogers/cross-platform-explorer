---
id: CPE-1813
title: TAR does not deliver the no-link-support refusal that ZIP does, so the two formats still disagree
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

CPE-1759 made a filesystem that cannot hold symlinks — a FAT stick, say — produce a **counted refusal**
rather than a dead extraction. That refusal is delivered for **ZIP only.**

For TAR, `unpack_in` creates the link itself and its error **aborts** (`crates/server/src/archive.rs:3318`);
`materialise_entry_symlink` is never on that path. So the very divergence CPE-1759 existed to close —
*the two formats answering the same situation differently* — survives in this one case.

## Why it matters

The rule CPE-1759 established is **refusals skip, failures abort**, and "this filesystem has no shortcuts"
is squarely a refusal: trying the next entry can plausibly work, and the other 499 files are still worth
having. TAR gives the user a dead extraction instead.

It is also the case most likely to be met in real life by someone doing something ordinary — extracting a
source tarball onto a USB stick.

## What to do

- Decide whether to route TAR's link creation through the same classifier, or to leave `unpack_in` in charge
  and translate its error. **Say which and why** — `unpack_in` owning the write is deliberate and CPE-1759
  already retracted one bad argument in this area, so do not re-derive that reasoning from the shape of the
  code.
- Whichever way, `WINDOWS_NO_LINK_SUPPORT` and POSIX `EPERM`/`EACCES` handling must end up **stated once**
  rather than duplicated per format — a second copy is how the two formats diverged in the first place.
- **Then fix the in-app help.** `src/docs/explorer-archives.md` should describe what actually ships. This
  file has carried a factually wrong statement repeatedly, so re-read the code before writing the sentence.
- Red-proof both formats in one test, the way CPE-1759's `one_shot_and_streamed_zip_answer_a_link_at_an_entry_name_identically`
  does — a per-format test cannot catch a divergence.

## Notes

Filed by the Foreman from the round-3 re-review of PR #958, 2026-08-20. Found because the reviewer checked
a doc sentence against the code rather than accepting it.

Related: **CPE-1759**, **CPE-1773/1774/1775**.

## Work Log

- 2026-08-20 — Implemented on branch `cpe-1813-tar-link-refusal`.

  **Decision (per "What to do", bullet 1): leave `unpack_in` in charge of the write and translate its
  error — not route TAR's link creation through `materialise_entry_symlink`.** `unpack_in` owning the
  write is deliberate (stated on `link_creation_is_categorical`'s doc and the module comment, both
  predating this ticket) and this ticket does not re-litigate it.

  **A real wrinkle surfaced doing the translation, measured rather than assumed: `tar-0.4.46`'s
  `Entry::unpack_in` throws away `raw_os_error()` when it wraps a link-creation failure** —
  `EntryFields::unpack`'s symlink/hard-link arms rewrap the syscall's `io::Error` via
  `Error::new(err.kind(), format!("{err} when …"))`, and `unpack_in` wraps *that* again via `TarError`.
  `Error::new` always builds a `Custom`-repr error, whose `raw_os_error()` is unconditionally `None` —
  confirmed with a standalone repro (`rustc`, not `cargo test`, to isolate it from this crate) before
  touching production code. A naive `link_creation_outcome(target, out, &e)` on `unpack_in`'s `Err`
  would therefore never match `WINDOWS_NO_LINK_SUPPORT`/`EPERM` — only the rarer `ErrorKind::Unsupported`
  arm — making the fix a no-op on exactly the FAT-stick case the ticket is about. Added
  `recover_raw_os_error` (walks the error's `source()` chain and parses `std::io::Error`'s own `"(os
  error N)"` Display text, which survives the wrap even though the typed code does not) and
  `tar_link_creation_outcome`, which reconstructs a `std::io::Error::from_raw_os_error(code)` when a code
  is recovered and feeds it to the **same** `link_creation_outcome`/`link_creation_is_categorical` ZIP
  uses — satisfying "stated once" (bullet 2) via reuse, not a second constant table. Wired into both
  `tar_unpack` (one-shot) and `extract_tar_stream` (streamed) at the `entry.unpack_in(...)` call site,
  gated on the entry being a symlink/hard-link (`tar_link_target`'s `Some`), so an ordinary file/dir
  write failure still aborts unchanged.

  Updated `src/docs/explorer-archives.md` (bullet 3): the "ZIP-only" / "failure in a TAR" claims in the
  *Safety limits* and *What changed recently* sections now describe the shared behaviour, with the old
  claim kept as a dated "this used to say" note rather than deleted, matching this file's convention.

  Updated the in-code doc comments that stated the ZIP-only claim as fact: the module-level history
  comment, `link_creation_is_categorical`'s own doc, and `tar_unpack`/`extract_tar_stream`'s docs.

  **Tests (bullet 4), both new, both in `archive.rs`:**
  - `cpe1813_recover_raw_os_error_reads_the_code_tar_rewrapped_away` — pure; wraps a raw OS error the
    same two-level way `tar-0.4.46` does (`wrap_like_tar_unpack_in_symlink_failure`, reproducing
    `entry.rs:529-568` and `TarError`'s `From` impl) and asserts the code round-trips.
  - `cpe1813_tar_and_zip_agree_on_the_no_link_support_refusal` — the "one test, both formats" the ticket
    asked for (bullet 4): for each representative OS code, compares `link_creation_is_categorical` on
    ZIP's direct error against `tar_link_creation_outcome` on the tar-wrapped shape of the same error,
    asserting agreement, plus a control leg for genuine failures (EACCES/ERROR_ACCESS_DENIED) that must
    abort on both, and a leg for the `ErrorKind::Unsupported` fallback when no code is recoverable.

  **Red-proof:** temporarily made `recover_raw_os_error` return `None` unconditionally (the exact defect
  this ticket fixes — reachable if `unpack_in`'s error were translated without code recovery). Both new
  tests failed: `cpe1813_recover_raw_os_error_reads_the_code_tar_rewrapped_away` on the round-trip
  assertion (`left: None, right: Some(1)`), and `cpe1813_tar_and_zip_agree_on_the_no_link_support_refusal`
  on the divergence assertion (`ERROR_PRIVILEGE_NOT_HELD (os error 1314)... ZIP says true; TAR says
  false`), i.e. an aborted extraction where ZIP would have skipped — the exact bug. Reverted after
  observing the red; `git diff --numstat` confirms no stray leftovers.

  **Gates:** `cargo clippy --manifest-path crates/server/Cargo.toml --all-targets -- -D warnings` clean;
  `cargo test --manifest-path crates/server/Cargo.toml --lib` — 2245 passed, 0 failed, 4 ignored (full
  crate, not just `archive::`). `src-tauri` and `src/` untouched, so their gates were not run. No
  `specta::Type` struct touched, so no bindings regen needed.

  **Not verifiable on this machine:** the actual FAT-stick/no-Developer-Mode triggers, same as every
  other test in this classifier family (`cpe1759_link_creation_separates_a_categorical_refusal_from_a_failure`'s
  own doc explains why no CI leg can stage them). The new tests reproduce the *shape* of the error each
  format hands our code, not the live trigger — consistent with how CPE-1759 tested the same classifier.
