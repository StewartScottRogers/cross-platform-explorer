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

- 2026-08-20 — **Round 2 rework**, on PR #965, after independent Reviewer round 1 returned CHANGES
  REQUESTED with three blockers (one a real security hole) plus a minor and a docs note. All addressed
  on the same branch.

  **Blocker 1 (security) — closed.** `recover_raw_os_error` (round 1) scraped `e.to_string()` first,
  which is `unpack_in`'s own outermost `TarError::desc`, `"failed to unpack `{file_dst}`"` — entirely the
  archive's own attacker-controlled entry path. The Reviewer's repro: a symlink entry named
  `payload (os error 1)` recovered code 1 (in `WINDOWS_NO_LINK_SUPPORT`) from a genuine
  `ERROR_ACCESS_DENIED` (5) failure, converting a real write failure into a silent skip. Fixed two ways,
  independently: (a) `e` itself is never inspected any more — only `e.source()` onward, where tar's own
  wrap text actually lives; (b) `parse_os_error_code` now requires the matched text to `starts_with` what
  `Error::from_raw_os_error(code)` itself renders, so a match cannot be forged by attacker text appearing
  later in a longer string. Confirmed each is independently load-bearing: reverting either alone still
  leaves the new regression test green; reverting *both* together (reproducing round 1's exact code)
  turns `cpe1813_a_crafted_entry_name_cannot_forge_a_link_support_refusal` red on the precise repro.

  **Blocker 2 (parity) — closed.** Renamed `recover_raw_os_error` → `recover_link_syscall_error`, which
  now only trusts a code/kind read off the ONE `source()`-chain level whose text is anchored to a literal
  marker (`" when symlinking "` / `" when hard linking "`, new consts `TAR_SYMLINK_MARKER`/
  `TAR_HARDLINK_MARKER`) — the exact text only `tar`'s own symlink/hard-link syscall wrap produces.
  `ensure_dir_created` (parent-directory creation) and `set_symlink_file_times` (the mtime set that runs
  *after* a symlink already exists on disk) both wrap their raw, unreformatted `io::Error` with no such
  marker anywhere, so a genuine `EPERM` from either is now correctly aborted rather than misfiled as
  "this volume has no links" — pinned by
  `cpe1813_a_parent_dir_or_mtime_failure_is_never_a_link_support_refusal`, which reproduces both wrap
  shapes with a categorical code and asserts abort.

  **Blocker 3 (test adequacy) — closed.** Per the Foreman's follow-up: the UAT tester confirmed this
  machine (Developer Mode on) and CI cannot stage a live 1314/FAT-stick refusal, so a probe-and-skip test
  would self-skip everywhere and prove nothing. Added an injection seam instead — `tar_unpack`/
  `extract_tar_stream` are now thin wrappers over `tar_unpack_with`/`extract_tar_stream_with`, which take
  the per-entry unpack operation as a parameter (defaulting to real `Entry::unpack_in`). Two new
  end-to-end tests build a REAL 3-entry tar (`a.txt`, a symlink `b`, `c.txt`) and inject a controlled,
  tar-shaped `Err` only at `b`, letting every other entry flow through the real, unmodified production
  loop. Mutation-proved exactly as asked: reverting `tar_unpack_with`'s routing to the pre-CPE-1813
  unconditional `return Err(e.to_string())` turns `cpe1813_tar_unpack_routes_a_link_creation_refusal_through_the_shared_classifier`
  red while the `extract_tar_stream` test stays green; reverting `extract_tar_stream_with`'s routing the
  same way turns the other one red while the `tar_unpack` test stays green. Both reverted after
  confirming red.

  **Minor 4 (message parity) — addressed.** `tar_link_creation_outcome`'s doc now states explicitly why
  the displayed message is the genuine syscall text rather than a synthesized one: a parsed code is
  redisplayed via `Error::from_raw_os_error(code)`, whose `Display` is the same deterministic OS-strerror
  lookup that produced the original wrapped text (same code, same platform ⇒ same string); the
  `Unsupported`-kind fallback (no parseable code) keeps the *exact* original prefix text verbatim rather
  than a generic message.

  **Docs note — no change needed.** The Foreman flagged that `src/docs/explorer-archives.md`'s "anything
  else about the write failing... still stops the extraction" sentence would be false under blockers 1/2.
  With both fixed, it is true as written (verified: a parent-dir EPERM and a forged-name attack both now
  abort, not skip) — the code was fixed rather than the sentence weakened, per direction.

  **Gates (round 2):** `cargo clippy --manifest-path crates/server/Cargo.toml --all-targets -- -D
  warnings` — clean. `cargo test --manifest-path crates/server/Cargo.toml --lib` — 2249 passed, 0 failed,
  4 ignored (net +4 tests vs. round 1: two round-1 tests renamed/updated for the new function signatures,
  four new tests added for blockers 1–3).
