---
id: CPE-1830
title: the Windows placeholder identity check cannot resist a deliberate attacker, and is itself TOCTOU
type: task
priority: Low
status: Backlog
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

- [ ] Windows identity is established from an **open handle**, not from a path stat — the route
      CPE-1765's own doc identifies: `CreateFileW` + `GetFileInformationByHandle`, comparing
      `dwVolumeSerialNumber` + `nFileIndexHigh`/`nFileIndexLow`. `batch_media` is the in-repo precedent
      for calling it; follow that pattern rather than inventing one.
- [ ] The verdict and the removal act on the **same handle**, so nothing can be substituted between
      them. If Windows semantics make that impossible, say so explicitly and state what bound survives.
- [ ] No new dependency (lean-core guardrail). If a hand-rolled `extern "system"` declaration is needed,
      its signature is checked against the real Win32 API — a wrong signature is memory corruption, not
      a bug — and CPE-1765's current zero-`unsafe` profile is noted as being given up deliberately.
- [ ] A test forges both timestamps on an empty impostor via `FileTimesExt` and asserts the drop leaves
      it alone. Red-proof it against the current check, which must fail that test today.
- [ ] CPE-1765's honest-bound wording in `fsutil.rs` is replaced with what actually ships.

## Notes

Filed from the CPE-1765 Security Auditor's round-3 FOLLOW-UP 1, which flagged that the deferral was
documented in the code but had no ticket — "an unticketed residual is the same failure with better
prose". Sibling deferrals from the same review: CPE-1825 (tree copiers), CPE-1826 (MotW through a
handle).
