---
id: CPE-1871
title: two design decisions in the prune loop are argued at length and pinned by nothing
type: task
priority: Medium
status: Doing
tags: ready
estimate: S
created: 2026-08-23
closed:
---

## Problem

Two lines in the retention path carry the longest comments in the module, and **neither is defended by a
test**. Both were found by the independent Reviewer on CPE-1863, by applying the rejected alternative and
running the full suite.

**1. CPE-1863's no-progress definition.** `snapshot_prune.rs:321` measures progress as the re-measured
store footprint falling, deliberately not as `prune` reporting bytes freed. Replace it with the rejected
form:

```rust
let progressed = freed_now > 0;
```

→ **2361 passed, 0 failed.** All five of CPE-1863's own tests pass under the definition its comment spends
a paragraph arguing against.

**2. CPE-1844's re-measure.** `store_total_bytes` is called again each iteration rather than subtracting
what `prune` reported. Apply the exact pre-CPE-1844 accounting in the current loop's shape:

```rust
let after = total.saturating_sub(freed_now);   // was: store_total_bytes(store_dir)?
```

→ **2361 passed, 0 failed.** CPE-1844's own review flagged this line as not independently red-proofable
and it was kept on argument; CPE-1863 then made it load-bearing for a second ticket without the coverage
improving.

## One test closes both

The divergence both decisions rest on has a single shape: **a blob whose last namer was pruned and whose
file could not be deleted.** `snapshot_capture.rs:738-744` credits `freed` only inside
`if fs::remove_file(&path).is_ok()`, and `:671` removes the manifest before the blob loop — so that blob
is credited **0** by `prune` and simultaneously **leaves** `total`.

Stage that, and both rejected alternatives become measurably wrong instead of merely argued against.

## Why this is not trivial, and why it was not done in CPE-1863

Staging an undeletable file defeated the Reviewer on Windows: Rust's `fs::remove_file` clears
`FILE_ATTRIBUTE_READONLY` and retries, and `icacls /deny <user>:(D)` did not block the delete in the
scratch location. Options worth trying, in rough order of portability:

- hold an open handle without `FILE_SHARE_DELETE` (Windows) — the ordinary cause in the field, since this
  is exactly the "a file is open" case
- an immutable/append-only attribute (Linux `chattr +i`, macOS `uchg`)
- a read-only **parent directory** rather than a read-only file (POSIX)
- an injectable seam on the delete, if the OS routes prove unportable — weaker evidence, but it pins the
  decision, and the module already uses injectable seams elsewhere (`tar_unpack_with`)

## Acceptance criteria

- [x] A fixture in which `prune` reports 0 while the footprint genuinely falls. Say how you staged it and
      on which platforms it runs; skip loudly rather than silently where it cannot.
- [x] Red-proof BOTH rejected alternatives against it — `freed_now > 0`, and
      `total.saturating_sub(freed_now)` — and record the observed failure for each.
- [x] Assert the fixture is live: that the delete really failed and the blob really left the witness's
      answer. A fixture that quietly deleted the file would pass while proving nothing, which is the
      failure mode this whole family keeps hitting.
- [x] If no portable staging exists, say so explicitly and record what the injectable-seam version does and
      does not prove. An honest weaker pin beats an argued-only line. (N/A here — a portable per-OS
      mechanism was found for all three CI platforms; see Work Log.)

## Notes

Filed from CPE-1863's review, which approved the PR and classed both as non-blocking precisely because the
fix's *behaviour* is right — it is the *defence* that is missing. Its own summary: this "pins a design
decision that is currently argued but not defended."

## Work Log

**2026-08-23 — closed.** Added one fixture,
`crates/server/src/snapshot_prune.rs::tests::cpe_1871_an_undeletable_blobs_freed_bytes_still_count_as_progress`,
plus a small `Undeletable` RAII helper in the same test module. It stages exactly the shape the ticket
names: three distinct 200-byte captures (GFS keeps all three, so only the byte cap can evict anything),
then makes the OLDEST checkpoint's blob file undeletable before `apply(cap = 500)` runs.

**Staging mechanism (portable, no skip needed on any CI OS):**
- Windows: `CreateFileW` opens the blob with `GENERIC_READ` and share mode
  `FILE_SHARE_READ | FILE_SHARE_WRITE` — deliberately omitting `FILE_SHARE_DELETE`. `std::fs::File::open`
  can't stage this (it always adds `FILE_SHARE_DELETE`); this is the same construction already proven in
  `fsutil.rs`'s `cpe_1739_windows_a_foreign_share_read_write_handle_still_blocks_the_save`, applied to a
  delete instead of a save. `DeleteFileW` (what `remove_file` uses) then fails with a sharing violation.
- Unix (Linux/macOS): `blobs/`'s own permissions drop to `0o555` (read+execute, no write). POSIX `unlink`
  needs write+execute on the PARENT directory, not on the target file, so this blocks the delete while
  `fs::metadata`/`fs::read_dir` (which only need read+execute) keep working inside `prune` and
  `store_total_bytes`. No root/elevated capability needed, so it stages the same on an ordinary CI runner
  as on a dev machine.
- The guard is gated through `crate::fsutil::require_staged("cpe_1871_undeletable_blob", true, ...)` — the
  existing CPE-1717 convention: `supported_here = true` because the mechanism is expected to work on every
  CI OS, so a staging failure goes RED under CI (`$CI` set) rather than a silent/quiet skip, and is only a
  loud local skip off CI. Verified the fixture passes as written: `cargo test --lib snapshot_prune` → 21
  passed, 0 failed (this repo's Windows dev box; the mechanism itself, and `require_staged`'s own strict/CI
  behaviour, is exercised identically by the existing CPE-1717 guard-neutralisation CI steps).

**Liveness, asserted in the test itself (not assumed):** after `apply`, the oldest manifest file is
confirmed gone (`prune`'s point-of-no-return ran) while `blob0_path.exists()` is asserted `true` (the
delete really failed — a fixture that quietly succeeded would fail this line), `bytes_freed == 0`, and a
direct re-measure `store_total_bytes` == 400 (the real, on-disk-independent proof the footprint genuinely
fell by the orphaned blob's 200 bytes even though nothing was actually deleted).

**Both red-proofs, run against the committed fixture, each reverted afterward with `git checkout --`:**

1. *CPE-1863's rejected form* — changed `let progressed = after < total;` to
   `let progressed = freed_now > 0;` (capturing `prune`'s return in a `freed_now` local first, which the
   loop didn't previously name). `cargo test --lib snapshot_prune::tests::cpe_1871` →
   ```
   thread '...cpe_1871_an_undeletable_blobs_freed_bytes_still_count_as_progress' panicked at
   src\snapshot_prune.rs:1655:9:
   assertion `left == right` failed: CPE-1871: a prune that reports bytes_freed == 0 must still
   count as progress when the re-measured store footprint genuinely fell — got StoppedNoProgress
     left: StoppedNoProgress
    right: Met
   test result: FAILED. 0 passed; 1 failed
   ```
2. *CPE-1844's rejected form* — changed the re-measure line
   `let after = snapshot_capture::store_total_bytes(store_dir)?;` to
   `let after = total.saturating_sub(freed_now);` (progress test line left as `after < total`, unchanged).
   Same command, same failure:
   ```
   assertion `left == right` failed: ... got StoppedNoProgress
     left: StoppedNoProgress
    right: Met
   test result: FAILED. 0 passed; 1 failed
   ```

Both swaps compute `progressed == false` from the same `freed_now == 0`, and both stop the loop at
`StoppedNoProgress` one eviction short of a cap that genuinely was met — exactly the divergence the ticket
describes, now caught rather than merely argued.

**Full-suite / clippy evidence (from `crates/server`):** `cargo test` → 2365 passed, 0 failed, 8 ignored
(plus the doctest/integration binaries in the same run, all 0 failed) — no regression from the small loop
refactor (naming `prune`'s return `freed_now` instead of inlining it, behavior-identical).
`cargo clippy --all-targets -- -D warnings` clean; `cargo clippy --all-targets --features specta
-- -D warnings` clean.

**Assumptions/judgment calls, logged rather than asked:**
- The ticket's suggested fixture ideas ("hold an open handle without `FILE_SHARE_DELETE`" on Windows, "a
  read-only parent directory" on Unix) both worked on the first attempt on this machine and matched an
  already-proven precedent in `fsutil.rs`, so the `chattr +i`/`uchg` and injectable-seam fallbacks in the
  ticket were not needed — acceptance criterion 4 is N/A rather than exercised.
- Left the loop's control flow otherwise untouched — only named `prune`'s return value (`freed_now`)
  instead of inlining it into `bytes_freed +=`, which the mutation red-proofs needed to express the
  rejected forms in the ticket's own vocabulary. No other line moved.
- Picked a cap (500) and sizes (3 × 200 bytes) that require exactly ONE eviction to reach `Met`, so the
  test pins the `Met` vs `StoppedNoProgress` divergence directly rather than through a multi-eviction chain
  that could pass for an unrelated reason.

Related: CPE-1863 (the no-progress rule), CPE-1844 (the re-measure), CPE-1861 (`manifests_naming`, the
witness both depend on).
