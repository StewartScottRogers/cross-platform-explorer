---
id: CPE-1755
title: A save to a free name lands 0600, and two doc lines describe the mode carry inaccurately
type: task
priority: Low
status: Done
tags: ready
estimate: XS
created: 2026-08-15
closed: 2026-08-18
---

## Problem

Observed by the PR #913 (CPE-1739) round-2 reviewer with `strace`, and explicitly judged non-blocking —
"worth folding into the next touch of this function".

CPE-1739 made the staging file be **created** at `0600` (closing a window where a private file was briefly
world-*openable* while being staged), then `fchmod`s it to the target's real mode. That fix is correct. Two
loose ends came with it:

### 1. A save to a brand-new name now lands `0600`

```
openat(AT_FDCWD, ".../brand-new.json.6653-....cpe-tmp", O_WRONLY|O_CREAT|O_EXCL|O_CLOEXEC, 0600) = 3
rename(".../brand-new.json.6653-....cpe-tmp", ".../brand-new.json")
```

No `fchmod` — `existing` is `None`, so `carry_protections` never runs and nothing widens it. Previously
such a file landed at the umask default (typically `0644`).

**Unreachable from production today**: `metadata_write_impl` reads the file before writing, so the target
always exists, and `write_file_text`'s Save-As does not use this function. It also errs on the **safe**
side. But the free-name path is deliberately supported and has its own test, so the behaviour should be
either intended-and-documented or changed.

Decide which: leaving it `0600` is defensible (a new file created by a tool arguably should not be
world-readable by default), but it is a silent departure from what every other file-creating path in the
app produces. Whichever way, say so at the site.

### 2. Two doc lines are inaccurate

- `STAGING_MODE`'s doc says the `fchmod` "only ever **widens** it to whatever the user's own file actually
  had". Not true for a `0400` target — there it **narrows**.
- The same doc does not cover the case where there is no user file at all (item 1 above).

## Acceptance criteria

- [ ] The brand-new-file mode is a recorded decision — either documented as intended at
      `create_staging_file`/`STAGING_MODE`, or changed to match the platform default, with the reason
      written down.
- [ ] `STAGING_MODE`'s doc describes what the `fchmod` actually does in all three cases: widen (`0644`
      target), narrow (`0400` target), and no target at all.
- [ ] No behaviour change to the carry itself — CPE-1739's narrow-then-widen ordering, its umask-controlled
      staging-mode test, and the `strace`-verified absence of a `0666` creation must all still hold. If item
      1 is changed rather than documented, the staging file must still be **created** at `0600`; only the
      final mode of a brand-new file may differ.

## Notes

Related: CPE-1739 (PR #913). The reviewer's round-1 blocker on that PR was the disclosure window this
`0600`-at-creation change closed; do not undo it while addressing item 1.

## Work Log

**Decision on item 1: documented as intended, not changed.** A brand-new-name save through
`stage_and_replace` stays at `STAGING_MODE` (`0600`) rather than being widened to the platform's
`0666 & ~umask` default. Recorded at the call site — the (deliberately absent) `else` of the
`carry_protections` call in `stage_and_replace` — and in `STAGING_MODE`'s doc. Reasoning:

- A private mode is the safer of the two defensible answers for a file this app writes on the user's
  behalf, and the staging file is *already* sitting at `0600` for CPE-1739's reasons — leaving it there
  costs nothing.
- Matching the platform default would mean deriving "the platform default" after the fact, which on Unix
  means calling `umask(2)` — the *only* way POSIX exposes the umask is by atomically **setting** it and
  reading back the old value, a global, process-wide, thread-unsafe side effect for a process that may be
  juggling other concurrent saves — a real cost for a path that does not even run in production today.
- Low stakes to get "wrong" either way: `metadata_write_impl` always reads the target before writing (so
  the target exists and this branch never runs from there), and `write_file_text`'s Save-As does not call
  this function at all. The free-name path is reachable only from tests
  (`cpe_1739_a_save_to_a_free_name_still_creates_the_file`, and this ticket's own
  `cpe_1755_a_brand_new_name_lands_at_the_0600_staging_birth_mode_not_the_platform_default`).
- If a future caller genuinely wants a brand-new file at the platform default, `create_exclusive` is
  already that entry point — no need to make `stage_and_replace` do double duty.

**`STAGING_MODE`'s doc rewritten** to describe all three cases (widen / narrow / no target at all)
instead of the old "only ever widens" line, which was true only by accident (every case CPE-1739 itself
exercised happened to widen).

**No behaviour change to the carry itself.** `carry_protections`, `carried_mode`, the narrow-then-widen
ordering, and the `0600`-at-`openat` creation are all untouched — only doc comments changed there, plus
one new (never-taken-on-`None`) doc comment at the call site. `create_staging_file` and `STAGING_MODE`'s
constant value (`0o600`) are unmodified.

**The three mode cases, measured on real IO** (`crates/server/src/fsutil.rs`, ran inside a Linux
container — this dev machine is Windows and these are `#[cfg(unix)]`-gated, so they cannot run here
natively; used Docker's `rust:1.90` image against this worktree, run as a non-root user so permission
checks are real):

- **Widen** — `0644` target: staged file born `0600`, `carry_protections` widens it to `0644`. Observed
  final mode `0644` (octal `420` in the assertion's decimal `left`/`right` — see raw output below).
- **Narrow** — `0400` target: staged file born `0600`, `carry_protections` narrows it to `0400`
  (owner-write bit removed). Observed final mode `0400`.
- **No target at all** — brand-new name: staged file born `0600`, `carry_protections` never runs.
  Observed final mode `0600` — the recorded decision above.

```
running 3 tests
test fsutil::tests::cpe_1755_a_brand_new_name_lands_at_the_0600_staging_birth_mode_not_the_platform_default ... ok
test fsutil::tests::cpe_1755_a_0400_target_narrows_the_staged_file_from_its_0600_birth_mode ... ok
test fsutil::tests::cpe_1755_a_0644_target_widens_the_staged_file_from_its_0600_birth_mode ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 2204 filtered out; finished in 0.03s
```

**New tests, each actually broken and re-run to prove it bites** (mutation applied locally, never
committed; reverted immediately after capturing the red output below):

1. `cpe_1755_a_0644_target_widens_the_staged_file_from_its_0600_birth_mode` and
   `cpe_1755_a_0400_target_narrows_the_staged_file_from_its_0600_birth_mode` — mutated
   `carry_protections` to always set the staged file's mode to `STAGING_MODE` (`0600`) instead of the
   real `carried_mode(...)` result (i.e. `carry_protections` becomes a no-op on the mode). Both reds:

   ```
   thread 'fsutil::tests::cpe_1755_a_0644_target_widens_the_staged_file_from_its_0600_birth_mode' panicked at src/fsutil.rs:3643:9:
   assertion `left == right` failed: a 0644 target must come back 0644, not stay at the staging file's 0600 birth mode (save result: Ok(()))
     left: 384
    right: 420

   thread 'fsutil::tests::cpe_1755_a_0400_target_narrows_the_staged_file_from_its_0600_birth_mode' panicked at src/fsutil.rs:3680:9:
   assertion `left == right` failed: a 0400 target must come back 0400 — narrower than the staging file's own 0600 birth mode (save result: Ok(()))
     left: 384
    right: 256

   test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 2204 filtered out; finished in 0.01s
   ```

   (`384` = `0600`, `420` = `0644`, `256` = `0400` — the mutation pinned the staged file's mode at its
   `0600` birth value in both cases, which is exactly what "only ever widens" quietly assumed away.)

2. `cpe_1755_a_brand_new_name_lands_at_the_0600_staging_birth_mode_not_the_platform_default` — mutated
   `stage_and_replace` to add an `existing.is_none()` branch that `set_permissions`-widened the staged
   file to `0644` before writing bytes, simulating "match the platform default instead". Red:

   ```
   thread 'fsutil::tests::cpe_1755_a_brand_new_name_lands_at_the_0600_staging_birth_mode_not_the_platform_default' panicked at src/fsutil.rs:3724:9:
   assertion `left == right` failed: a save to a free name has nothing to carry and must stay at the staging file's 0600 birth mode (CPE-1755's recorded decision); save result: Ok(())
     left: 420
    right: 384

   test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 2204 filtered out; finished in 0.01s
   ```

Both mutations were reverted in the same session before committing; `git diff --stat` after reverting
matched the pre-mutation diff exactly (`crates/server/src/fsutil.rs | 161 ++, 4 -`).

**Cleanup**: all three new tests arm a `Drop`-guard `struct Cleanup<'a>(&'a Path)` (the
`split_join.rs`/`dispatch.rs` pattern) *before* the panicking assertion, so a red run still removes its
scratch directory.

**Full verification, in a Linux container (non-root user) against this worktree**:
- `cargo test` (default features): `2203 passed; 0 failed`. (An earlier run as container **root** showed
  one unrelated failure, `snapshot_capture::tests::scan_skips_unreadable_files_without_failing_the_whole_scan`
  — confirmed to be root bypassing the Unix permission check the test relies on, not a real regression;
  re-ran as uid 1000 and it passed along with everything else.)
- `cargo clippy --all-targets -- -D warnings` and `cargo clippy --all-targets --features index -- -D
  warnings`: both clean for `fsutil.rs`. One **pre-existing, unrelated** clippy error surfaced crate-wide
  in `src/media_meta_write.rs:2195` (`clippy::search_is_some`) under this container's clippy (a newer
  `stable` than whatever CI's `dtolnay/rust-toolchain@stable` resolved to on `main`'s last green run,
  commit `3a8baf82`, 2026-08-18T04:40Z) — confirmed pre-existing by `git stash`-ing this ticket's entire
  diff and reproducing the identical error on unmodified `main`; not touched by this ticket (scope is
  `fsutil.rs` only). Verified my own diff is clippy-clean in isolation by temporarily (locally, uncommitted,
  reverted immediately) applying clippy's own suggested one-line fix to that unrelated line and re-running
  clippy: zero warnings anywhere, both feature modes.
- CPE-1739's own tests (`cpe_1739_a_save_carries_the_mode_so_a_private_file_stays_private`,
  `cpe_1739_the_staging_opener_creates_a_file_no_one_else_can_open`,
  `cpe_1739_a_save_to_a_free_name_still_creates_the_file`,
  `cpe_1739_classify_carryover_refuses_a_target_it_cannot_read_but_allows_an_absent_one`,
  `cpe_1739_carried_mode_keeps_every_bit_but_drops_setuid_when_the_owner_changes`) all still pass
  unmodified — confirms the narrow-then-widen ordering, the umask-controlled staging-mode probe, and the
  free-name-still-creates behaviour are all intact.
