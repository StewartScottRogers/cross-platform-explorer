---
id: CPE-1813
title: TAR does not deliver the no-link-support refusal that ZIP does, so the two formats still disagree
type: bug
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-20
closed: 2026-08-20
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

- 2026-08-20 — **Round 3 rework (attempt 3/3)**, on PR #965, after independent Reviewer round 2 verified
  round 2's fixes itself (both mutations run, injection seam praised as the right call) and returned two
  new BLOCKING findings, both cheap, both found by the Reviewer auditing the crate source directly rather
  than accepting round 2's own enumeration.

  **Finding 1 (round 2's own blocker-2 fix modelled the wrong nesting for its `set_symlink_file_times`
  example) — closed, structurally rather than with another phrase to anchor on.** Round 2's
  `wrap_like_tar_single_level_failure` modelled the mtime failure as ONE `TarError` wrap (correct for
  `ensure_dir_created`, wrong for mtime). The real chain is two levels:
  `outer(TarError "failed to unpack") -> mid(TarError "failed to set mtime for `{dst}`") -> raw`. `mid`'s
  own rendered text embeds `{dst}` — the entry's own attacker-controlled destination path — and round 2's
  marker search (`text.find(marker)`) read `mid` directly, so an entry named `a when symlinking b` put the
  marker inside `mid`'s text with an `Unsupported`-kind mtime failure behind it; round 2's `starts_with`
  hardening protected the CODE arm but the KIND-only fallback arm had no equivalent guard, so this
  recovered `Some(io::Error{kind: Unsupported, ..})` and reported a symlink that already existed on disk
  as a skip. Reproduced exactly (Reviewer's own numbers) before fixing.

  Fixed structurally, not by enumerating more `TarError::desc` phrases: `recover_link_syscall_error` now
  only trusts a `source()`-chain level that is a LEAF — has no `source()` of its own. `TarError::source`
  unconditionally returns `Some(&self.io)`, so `mid` (and `ensure_dir_created`'s TarError wrapper) are
  excluded on sight, regardless of wording, and the walk passes straight through to the raw OS error
  underneath, which never carries the entry's name. This holds for every current and future
  `TarError::desc` site without enumerating them — closing the actual gap the Reviewer's finding was
  "your enumeration is not exhaustive" about.

  **Audited beyond the finding, per the Reviewer's own method (grepped every `Error::new(kind,
  format!(..))` site in `entry.rs`) rather than trusting round 2's list:** `validate_inside_dst`'s
  hard-link leg (`entry.rs:543`) is ALSO a leaf-shaped wrap — `"{err} while canonicalizing {attacker
  hard-link target}"` — so the leaf check alone does not exclude it. Added a second, narrow guard:
  a level's prefix is never trusted if it contains `" while canonicalizing "`. Narrower trigger than the
  mtime hole (needs a hard-link entry whose declared target does not resolve), but excluded on the same
  principle rather than left as a theoretical gap the Reviewer would have found next round.

  Also corrected the two marker-doc line citations the Reviewer flagged as approximated rather than
  measured (`entry.rs:573` for the symlink wrap, `:552` for the hard-link wrap — not `559-568`/`544-553`),
  scoped the `TarError::desc` enumeration explicitly to the sites reachable on a link entry's file/
  symlink/hard-link arms (naming the ones that are NOT reachable rather than omitting them), and fixed
  two stale `[`recover_raw_os_error`]` doc references left over from round 2's rename to
  `recover_link_syscall_error`.

  New tests: `cpe1813_an_mtime_failure_named_to_embed_the_marker_is_never_a_link_support_refusal`
  (the Reviewer's exact repro, against the corrected two-level `wrap_like_tar_mtime_failure` fixture) and
  `cpe1813_a_canonicalize_failure_named_to_embed_the_marker_is_never_a_link_support_refusal` (the
  self-found `validate_inside_dst` case). `wrap_like_tar_single_level_failure` is now scoped to
  `ensure_dir_created` only; its own test renamed to `cpe1813_a_parent_dir_failure_is_never_a_link_support_refusal`.

  **Finding 2 (nothing pinned that the two call sites choose the right MARKER) — closed.** The Reviewer's
  own mutation — collapsing `let marker = if entry_type.is_hard_link() { .. } else { .. }` to always
  `TAR_SYMLINK_MARKER` at both `tar_unpack_with`/`extract_tar_stream_with` call sites — silently reverted
  every hard-link entry to the pre-CPE-1813 abort with the full suite green, because both e2e seam tests
  only exercised `EntryType::Symlink`. Fixed by parameterising the crafting fixture
  (`craft_tar_with_symlink_in_the_middle` → `craft_tar_with_link_in_the_middle(entry_type, target)`) and
  giving both `cpe1813_tar_unpack_routes_a_link_creation_refusal_through_the_shared_classifier` and
  `cpe1813_extract_tar_stream_routes_a_link_creation_refusal_through_the_shared_classifier` a
  `("hard link", tar::EntryType::hard_link(), TAR_HARDLINK_MARKER)` leg alongside the existing symlink one,
  injecting a `TAR_HARDLINK_MARKER`-shaped failure and asserting the same skip/counted/recorded contract.

  **Red-proofs, exactly as asked, each stated against the specific production change that reds it:**
  - Removing the leaf-check guard (`std::error::Error::source(level).is_none()` → unconditionally `true`)
    reds `cpe1813_an_mtime_failure_named_to_embed_the_marker_is_never_a_link_support_refusal` — observed
    the exact garbled `Ok(Some("...failed to set mtime for `dest/a). Skipped..."))` the Reviewer measured,
    then reverted.
  - Removing the `" while canonicalizing "` guard reds
    `cpe1813_a_canonicalize_failure_named_to_embed_the_marker_is_never_a_link_support_refusal` (had to
    fix this test's own fixture first — its original `NotFound`-shaped inner error aborted on the
    classifier's own merits regardless of the guard, so the test could not have failed either way; the
    guard is only reachable via the kind-only fallback, so the fixture now uses an `Unsupported`-kind
    inner error, matching the mtime test's own reasoning), then reverted.
  - Collapsing both call sites' marker selection to always `TAR_SYMLINK_MARKER` reds BOTH
    `cpe1813_tar_unpack_routes_a_link_creation_refusal_through_the_shared_classifier` and
    `cpe1813_extract_tar_stream_routes_a_link_creation_refusal_through_the_shared_classifier` — on the
    "hard link" leg specifically in each (the "symlink" leg stays green in both, confirming the mutation
    is caught precisely rather than by a broader collateral failure), then reverted.

  **A note on process:** an earlier attempt to apply the finding-1 fix via a large exact-string-match
  block replacement accidentally dropped two round-2 tests
  (`cpe1813_recover_link_syscall_error_reads_the_code_tar_rewrapped_away` and
  `cpe1813_a_crafted_entry_name_cannot_forge_a_link_support_refusal`) when a subsequent, unrelated block
  replacement's match boundary turned out wider than the diff review assumed. Caught by `grep -c "fn
  cpe1813"` before running the final gates, not by the test run alone (both surviving tests still passed
  a 6/6 run — the loss was silent). Both tests reinstated verbatim; `git diff` reviewed line by line
  afterward to confirm nothing else was lost.

  **Gates (round 3):** `cargo clippy --manifest-path crates/server/Cargo.toml --all-targets -- -D
  warnings` — clean. `cargo test --manifest-path crates/server/Cargo.toml --lib` — 2251 passed, 0 failed,
  4 ignored (net +2 tests vs. round 2: two new regression tests for finding 1's two sub-cases; finding 2
  added legs to two existing tests rather than new test functions).
