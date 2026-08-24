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

**2026-08-23 — Worker (fsutil.rs), round 2: Security Auditor finding (PR #1018, gauntlet attempt 2)**

The Security Auditor ran a 14-attack battery against the handle-identity fix above and found zero
bypasses (forged timestamps, hard link, file symlink, directory junction to `id_rsa`, planted directory,
ADS stash, shared-write byte injection, rename-away-with-hard-link-back, and a 400-iteration
delete-and-recreate race — attacker's `create_new` won 400/400, still refused). It also verified the
`unsafe` block against the vendored `windows` 0.56 source (signatures, NUL-termination, single-close,
struct size) and confirmed PR #1016 merges onto this branch clean, no conflict.

**Blocking finding: the removal itself changed deletion semantics from `main`.** `main` removed the
placeholder with `std::fs::remove_file`, which on Windows uses `FileDispositionInfoEx` with
`FILE_DISPOSITION_FLAG_POSIX_SEMANTICS` — unlinked immediately, even with other handles open. My
handle-identity fix used legacy `SetFileInformationByHandle(FileDispositionInfo, DeleteFile: TRUE)`,
whose unlink is deferred until the *last* handle closes. Any third party holding the placeholder open
across the drop — Defender's real-time scan, Windows Search indexing, Explorer's thumbnailer, a sync
client, another agent, **no attacker required** — left the name `DELETE_PENDING`: every open against it
answered `ERROR_ACCESS_DENIED`, breaking the cross-volume-move fallback's documented contract ("the
fallback finds the name as it left it and re-claims it atomically", `src-tauri/src/lib.rs`) on every
C:→D:, USB, and network-share move a third party happened to touch mid-flight.

**Fix:** `remove_placeholder_by_handle_identity` now tries `SetFileInformationByHandle(
FileDispositionInfoEx, FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS)` first —
immediate unlink regardless of other open handles, matching `remove_file`'s own Windows behaviour — and
falls back to the legacy `FileDispositionInfo` call only if that fails (a pre-1607 OS/filesystem). Both
calls still run on the one handle the identity verdict was just read from — the same-handle guarantee is
unchanged. `FILE_DISPOSITION_INFO_EX_FLAGS` has no `BitOr` impl in the vendored `windows` 0.56 crate
(unlike `FILE_ACCESS_RIGHTS`), so the two flag bits are combined via their raw `.0` values.

New test `cpe_1830_a_third_party_handle_open_across_the_drop_does_not_leave_the_name_undeletable`:
stakes a placeholder, opens a second ordinary read handle on it (the "outsider"), holds it open across
`drop(claim)`, then asserts the name is gone immediately and a fresh `create_new` reclaims it.

*Before (legacy `FileDispositionInfo` only — the code this same test was inserted into from the
previously-pushed commit `f7c54edb`):*
```
[CPE-1830 A14] name present after drop = true ; fallback create_new = Err(Os { code: 5, kind:
PermissionDenied, message: "Access is denied." })
test fsutil::tests::cpe_1830_a_third_party_handle_open_across_the_drop_does_not_leave_the_name_undeletable ... FAILED
test result: FAILED. 0 passed; 1 failed
```

*After (POSIX-semantics-first, this fix):*
```
[CPE-1830 A14] name present after drop = false ; fallback create_new = Ok(File { handle: 0x1a4, path: ... })
test fsutil::tests::cpe_1830_a_third_party_handle_open_across_the_drop_does_not_leave_the_name_undeletable ... ok
test result: ok. 1 passed; 0 failed
```

**Three documentation defects fixed in the same pass** (all flagged by the auditor as reader-facing, not
just cosmetic):

1. `SlotClaim::stake`'s doc measured `std::fs::remove_file` while our own handle is open, to show holding
   a handle does not confer exclusivity. That measurement is still true, but is now about an *unrelated
   third party's* `remove_file`, not about how `Drop` removes its own placeholder (which no longer calls
   `remove_file` at all on Windows) — the doc now says so explicitly and cross-references
   `remove_placeholder_by_handle_identity` for what `Drop` actually does and why it still achieves "free
   again immediately".
2. `Drop`'s doc read as if it covered the whole removal, but the **directory arm is unchanged** — still
   `symlink_metadata(path).is_dir()` then `remove_dir(path)`, by path, no handle, no identity. The doc now
   states plainly that the identity-then-remove mechanism is the file arm only, with the directory arm's
   own (pre-existing, unaffected) bound spelled out alongside it.
3. **Undisclosed behaviour change:** a degenerate identity (zero volume or file index — some network
   redirectors, CPE-1642 F2) now means the function deletes nothing, including its own untampered
   placeholder, where pre-fix it would have succeeded (identity was never being compared before). Stated
   explicitly in the "What is NOT closed" section as a deliberate fail-closed trade, not an oversight: the
   cost is a reclaim-refusing stub on such a share, never a wrong deletion. Also noted per the auditor: the
   real QNAP NAS test target is *not* degenerate (cleanup verified working there), and `is_degenerate` only
   catches *zero* — a hypothetical redirector reporting a constant non-zero index for every object would
   still collide, but the blast radius does not grow past CPE-1765's existing one-file bound.

**Re-verification:** `cargo clippy --all-targets -- -D warnings` clean (default and `--features index`);
`cargo test --lib`: 2368 passed, 0 failed, 8 ignored (pre-existing/unrelated) — one more test than round 1
(the new A14 regression test). All synchronous, no background polling, per Foreman's instruction that they
own CI for this PR.

**2026-08-23 — Worker (fsutil.rs), round 3: Security Auditor re-audit of `3b697737` (PR #1018, gauntlet attempt 3)**

The Security Auditor re-ran the full 14-attack battery against the shipped POSIX-semantics fix (not a
prototype): 14/14 denied, including the 400-iteration race. Confirmed the same-handle guarantee holds in
fact (both the Ex and legacy calls act on `current`; the Ex-failure path opens no second handle;
`CloseHandle` runs exactly once on every path). Two findings landed in this round, plus one live network
measurement worth keeping as a documented fact about this codebase rather than only in an agent transcript.

**Finding 1 (attack A15) — the Ex flags omitted `IGNORE_READONLY_ATTRIBUTE`, a measured regression.** One
unprivileged `SetFileAttributes` (`std::fs::set_permissions(..).set_readonly(true)`) on the placeholder
between `stake` and `Drop` — no race window, no attacker capability beyond seeing the file — made both the
Ex and legacy disposition calls refuse it, leaking the placeholder every time:

```
main            : readonly placeholder still present after drop = false   (removed)
branch 3b697737 : readonly placeholder still present after drop = true    (leaked)
```

`std::fs::remove_file`'s own Windows `posix_delete()` sets `FILE_DISPOSITION_FLAG_DELETE |
POSIX_SEMANTICS | IGNORE_READONLY_ATTRIBUTE` together, deliberately. Added the third flag (already
exported by the vendored `windows` 0.56 crate — no new dependency) to the same `FILE_DISPOSITION_INFO_EX`
this fix already builds. New test `cpe_1830_a_readonly_placeholder_is_still_removed_on_drop`, red-proofed
by inserting it into the previously-pushed `3b697737`:

*Before:*
```
[CPE-1830 A15] readonly placeholder still present after drop = true
test fsutil::tests::cpe_1830_a_readonly_placeholder_is_still_removed_on_drop ... FAILED
```
*After:*
```
[CPE-1830 A15] readonly placeholder still present after drop = false
test fsutil::tests::cpe_1830_a_readonly_placeholder_is_still_removed_on_drop ... ok
```

**Finding 2 — three doc sentences were false; corrected, not the code (parity, not a regression).**

**The SMB redirector refuses `FileDispositionInfoEx` outright — measured live against a real network
share.** The auditor issued the class by hand on each volume:

```
[local NTFS]            FileDispositionInfoEx: ACCEPTED  -> POSIX path
[\STEWART-NAS\Public]  FileDispositionInfoEx: REFUSED — The parameter is incorrect. (0x80070057)
[\STEWART-NAS\Public]  legacy FileDispositionInfo:      ACCEPTED (deferred unlink)
```

So the legacy fallback in `remove_placeholder_by_handle_identity` is not an antique-OS path, it is the
**network-share path** on a fully-patched Windows 11 host — reached on every move to a network
destination, one of the three this module's own docs already name (C:→D:, USB, network share), and the
A14 deferred-unlink exposure (a third party's handle open across the drop) persists there. This is
**parity with `main`, not a regression**: `std::fs::remove_file` gets the identical
`ERROR_INVALID_PARAMETER` refusal on the same share and falls back to nothing better. FAT/exFAT is very
likely the same (FastFAT does not implement the Ex class either) but was not tested, so the doc states
that as a likelihood, not a claim. No code change follows from this — the fallback already exists and is
correct; only what the comments claimed about *when* it is reached was wrong, now corrected at
`fsutil.rs` (the `remove_placeholder_by_handle_identity` doc block, and the `FileDispositionInfoEx`
version note — the `SetFileInformationByHandle` information class is Windows 10 **1709**, not 1607, which
is the unrelated NT-level `FILE_DISPOSITION_INFORMATION_EX` structure).

**The "exactly the fallback `std::fs::remove_file` itself takes" claim was also false**, verified against
the std source shipped with this toolchain: Windows `unlink` calls `DeleteFileW` first and only opens a
DELETE handle for `posix_delete()` after an `ERROR_ACCESS_DENIED`; its own legacy `win32_delete()` is
`#[allow(unused)]` dead code, never called, and a `DeleteFileW` failure is returned as-is with no second
attempt. `std` has no legacy fallback at all — neither the order nor the existence matched. The comment
now states plainly that falling back to legacy `FileDispositionInfo` here is this module's own choice,
not something modeled on `std`.

**`SlotClaim::stake`'s round-2 correction also overclaimed**, asserting unconditionally that the fix
preserves "exactly the same 'name free again immediately' property" — true only on local NTFS/ReFS; on a
network share neither this process's removal nor a third party's ordinary `remove_file` frees the name
immediately (per the SMB measurement above), so the underlying measurement in that doc block is now noted
as NTFS/local-disk-only.

Corrections (b) and (c) from round 2 were checked line-by-line by the auditor against the shipped code and
every factual claim held, including the QNAP result and the seam-injection behaviour — no change needed
there.

**Re-verification:** `cargo clippy --all-targets -- -D warnings` clean (default and `--features index`);
`cargo test --lib`: 2369 passed, 0 failed, 8 ignored (pre-existing/unrelated) — one more than round 2 (the
new A15 regression test). All synchronous, no background polling, per Foreman's instruction that they own
CI for this PR (attempt 3 of 3).
