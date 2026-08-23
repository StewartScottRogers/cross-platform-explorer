---
id: CPE-1830
title: the Windows placeholder identity check cannot resist a deliberate attacker, and is itself TOCTOU
type: task
priority: Low
status: Doing
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

`SlotClaim::drop` removes the placeholder it staked. To avoid deleting a file it did not create, it
verifies identity first — exactly `(dev, ino)` on Unix, but on Windows there is no stable
handle-identity API in `std`, so it falls back to: regular file, zero length, and matching created and
modified timestamps.

That fallback does not resist a deliberate local attacker, for two independent reasons, both measured
during the CPE-1765 review:

1. **Both timestamps are forgeable.** `std::os::windows::fs::FileTimesExt::set_created` and
   `set_modified` are stable, safe, and available to anyone who can write the file. An impostor that is
   an empty regular file with matching stamps passes:

   ```
   [R2] created matches? true   modified matches? true   len = 0, is_file = true
   [R2] impostor survived the drop? false     <- drop deleted a file it did not create
   ```

   NTFS tunnelling makes it easier still: re-measured 3/3, a delete-and-recreate at the same name inside
   the 15-second window restored **both** stamps, `mod_delta=Some(0ns)`.

2. **The check is itself TOCTOU.** `placeholder_is_still_ours` stats the **path**; `remove_file` then
   re-resolves the **path**. Nothing carries identity from the verdict into the removal.

## Why this is Low and not urgent

The failure direction is litter, never data: the removal is `remove_file` / `remove_dir` (never
`remove_dir_all`), so it is bounded to one file or one empty directory at the exact claimed path, and a
planted directory *link* is left alone entirely (verified). Length and type do the real work; the
timestamps only ever make the check stricter. CPE-1765 states this bound honestly rather than claiming
more — that wording is correct and should stay until this is closed.

Reachability is real though: the drop path runs on **every cross-volume move** — C: to D:, to a USB
stick, to a network share.

## Acceptance criteria

- [x] Windows identity is established from an **open handle**, not from a path stat — the route
      CPE-1765's own doc identifies: `CreateFileW` + `GetFileInformationByHandle`, comparing
      `dwVolumeSerialNumber` + `nFileIndexHigh`/`nFileIndexLow`. `batch_media` is the in-repo precedent
      for calling it; follow that pattern rather than inventing one.
- [x] The verdict and the removal act on the **same handle**, so nothing can be substituted between
      them. If Windows semantics make that impossible, say so explicitly and state what bound survives.
- [x] No new dependency (lean-core guardrail). If a hand-rolled `extern "system"` declaration is needed,
      its signature is checked against the real Win32 API — a wrong signature is memory corruption, not
      a bug — and CPE-1765's current zero-`unsafe` profile is noted as being given up deliberately.
- [x] A test forges both timestamps on an empty impostor via `FileTimesExt` and asserts the drop leaves
      it alone. Red-proof it against the current check, which must fail that test today.
- [x] CPE-1765's honest-bound wording in `fsutil.rs` is replaced with what actually ships.

## Notes

Filed from the CPE-1765 Security Auditor's round-3 FOLLOW-UP 1, which flagged that the deferral was
documented in the code but had no ticket — "an unticketed residual is the same failure with better
prose". Sibling deferrals from the same review: CPE-1825 (tree copiers), CPE-1826 (MotW through a
handle).

## Work Log

**2026-08-23 — Worker (fsutil.rs)**

**Threat model.** A local attacker who can create/delete/write files at the destination folder — a
zero-byte sweeper, AV quarantine, a sync client, or a deliberate adversary racing a cross-volume move —
controls the object present at the placeholder's picked name for the whole window between
`SlotClaim::stake` and its `Drop`. Before this fix they could unlink the real placeholder and recreate an
empty file at the same name, forge its `created`/`modified` timestamps to match the original via
`std::os::windows::fs::FileTimesExt` (stable, safe, requires only write access to the file they just
created), and the drop's identity check — comparing only shape and those two timestamps — would answer
"still ours" and delete the impostor. The check was also TOCTOU on its own terms: it stat'd the *path* for
the verdict and `remove_file` re-resolved the *path* again for the removal, so even a correct verdict
described a moment already gone. The blast radius stays what CPE-1765 already bounded — `remove_file`, one
file, never `remove_dir_all` — so the reachable harm is deleting one file (or leaving litter), not
exfiltrating or corrupting data; this ticket closes the "which file" question, not the "how much" one.

**What changed.** `placeholder_is_still_ours` is now Unix-only (still exact `(dev, ino)` from the open
descriptor; unaffected, its own doc now explains why a held-open fd already makes its two-resolution shape
harmless there). The Windows half is new: `remove_placeholder_by_handle_identity` opens the *current*
object at the path via `CreateFileW` (`FILE_FLAG_OPEN_REPARSE_POINT`, so a link at the name is opened, not
followed) with `DELETE | FILE_READ_ATTRIBUTES`, reads its `(dwVolumeSerialNumber,
nFileIndexHigh/nFileIndexLow)` identity, shape and length via `GetFileInformationByHandle` on **that same
handle**, compares it against the placeholder's own identity (read from the handle `SlotClaim` has held
open since `stake`, via the existing `crate::batch_media::handle_facts`, including its `is_degenerate`
guard against the zero-index value some network redirectors report for every object), and — only on an
exact, non-degenerate match — removes it via `SetFileInformationByHandle(FileDispositionInfo,
DeleteFile=TRUE)` on that **exact same handle**, then closes it. Verdict and removal never resolve the path
twice; nothing can be substituted at the name in between. `CreateFileW` + `GetFileInformationByHandle` is
the exact call `crate::batch_media::probe_no_follow`/`handle_facts` already make (reused, not
reimplemented); `SetFileInformationByHandle` is new to this module but is exported by the already-vendored
`windows` 0.56 crate (`Win32_Storage_FileSystem` feature, already enabled) — no new dependency, and no
hand-rolled `extern "system"` was needed since the crate already binds the Win32 signature. `SlotClaim`
keeping its placeholder handle open for the whole lifetime (unchanged by this ticket) is load-bearing for
both platforms' identity comparisons: it pins the underlying file record so the recorded `(volume, index)`
/ `(dev, ino)` cannot be reassigned to an unrelated new file while the comparison is made.

**AC2 ("same handle... if impossible, say so"): fully achievable, not partial.** Windows semantics did not
force a compromise here — `SetFileInformationByHandle` performs the removal on the identical handle
`GetFileInformationByHandle` just read the verdict from, so this is not a "closest bound" but the literal
same-handle guarantee the ticket asked for.

**Evidence — attack reproduced, then denied** (both runs against the same new test,
`cpe_1830_a_dropped_claim_never_deletes_a_timestamp_forged_impostor`, by temporarily swapping in the
pre-fix `fsutil.rs` from `origin/main` with only this test appended, run, then restoring the fix):

*Before (pre-fix `origin/main` code, test appended):*
```
thread 'fsutil::tests::cpe_1830_a_dropped_claim_never_deletes_a_timestamp_forged_impostor' panicked at
src\fsutil.rs:8192:9:
assertion `left == right` failed: dropping the claim deleted a timestamp-forged impostor it never created
  left: None
 right: Some([])
test fsutil::tests::cpe_1830_a_dropped_claim_never_deletes_a_timestamp_forged_impostor ... FAILED
test result: FAILED. 0 passed; 1 failed
```

*After (this fix):*
```
test fsutil::tests::cpe_1830_a_dropped_claim_never_deletes_a_timestamp_forged_impostor ... ok
test result: ok. 1 passed; 0 failed
```

**Also updated:** the module-level doc note near `copy_file_into_claimed_slot` about "the only re-check
available is `placeholder_is_still_ours`, forgeable and TOCTOU on Windows" — that function is now Unix-only
and the Windows mechanism is fixed, so the passage was rewritten to explain precisely why this fix does
*not* generalize to gating the removed Mark-of-the-Web carry feature (that would re-open the destination by
*path* after the fact, without the held-open-handle precondition this fix's safety rests on) rather than
leaving a stale claim in place.

**Assumption logged:** `crates/server/src/archive.rs`, the restore/backup write paths, and
`copy_tree_into_claimed_slot` are explicitly out of scope per the sprint's in-flight-work notice (other
workers own those files/paths) and were not touched; `claim_dir_slot`'s directory-slot removal path
(`remove_dir` + `is_dir` check, no handle involved) was left unchanged — the ticket's problem statement and
acceptance criteria are entirely about the **file** identity check, and the directory arm has no
timestamp/identity comparison to begin with (its guard is `symlink_metadata(&self.path).is_ok_and(|m|
m.file_type().is_dir())`, unaffected by anything this ticket measured).

**Verification:** `cargo clippy --all-targets -- -D warnings` clean (default features and `--features
index`); `cargo test` (default features) green, `crates/server` only — `fsutil` module: 89 passed, 0
failed, 1 ignored (pre-existing, unrelated). `src-tauri` untouched by this change (no call site there
needed edits).
