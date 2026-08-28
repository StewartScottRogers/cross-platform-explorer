---
id: CPE-1963
title: The staging rename's SOURCE is an enumerable, attacker-writable path — the commit can be aliased onto a file outside the root
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

Found by PR #1070's (CPE-1958) round-2 Security Auditor and **re-measured on the merged state** by that
PR's worker before filing. It is **pre-existing in `fsutil::stage_and_replace`** — the editor's save has
had it since CPE-1739 — but CPE-1958 newly routes the macro engine's **confirmed overwrite** through the
same staging function, so the confirmed path did not have this exposure before and now does.

`stage_and_replace_at` writes the user's bytes into a sibling it exclusively creates,
`<name>.<pid>-<nanos>.cpe-tmp`, and commits with `std::fs::rename(tmp, target)`.

`Commit::ReplacingTheName`'s invariant — *"nothing planted at the destination after the caller's checks
ran can redirect the commit"* — is **true of the destination and silent about the source**. `tmp` is a
**path**: an enumerable `*.cpe-tmp` directory entry sitting in the same attacker-writable folder as the
destination. An attacker that `readdir`s for it, unlinks it, and hard-links an outside victim into its
place makes the rename commit **the victim's inode** over the confirmed name.

## Measured, both platforms, on the merged state

`crates/server/src/fsutil.rs` → `cpe_1958_rename_source_report` (`#[ignore]`d; run with
`cargo test -p cpe-server cpe_1958_rename_source_report -- --ignored --nocapture`, and set
`TMPDIR` to a real local filesystem — a WSL `drvfs` mount is not valid). 3,000 trials per shape:

| shape | destinations aliased to the outside file | returned `Ok` WITHOUT writing the user's bytes | victim's CONTENT changed |
|---|---|---|---|
| relink an outside victim — **Linux ext4** | **2,834 / 3,000** | **2,834 / 3,000** | **0 / 3,000** |
| relink an outside victim — **Windows 11 / NTFS** | **6 / 3,000** | **6 / 3,000** | **0 / 3,000** |
| delete-only (CONTROL) — Linux ext4 | 0 / 3,000 | 0 / 3,000 | 0 / 3,000 |
| delete-only (CONTROL) — Windows 11 / NTFS | 0 / 3,000 | 0 / 3,000 | 0 / 3,000 |

The delete-only control is what tells this apart from "the rename fails sometimes": an attacker that only
unlinks the temp produces **zero** lying `Ok`s in 3,000 trials on either platform. The aliasing needs the
re-link.

**Reproduced independently (2026-08-27, PR #1070 round-3 Security re-audit).** A third party re-ran
`cpe_1958_rename_source_report` at 3,000 trials on its own ext4 root and got **2,685 aliased / 2,685
lying `Ok` / 0 victim-content changes**, with the delete-only control at **0**. That lands inside this
ticket's own Linux spread (2,783 / 2,834), from a harness run nobody here set up — so the defect, its
rate, *and* the "content never changes, the `Ok` lies" shape are all corroborated rather than
self-reported.

**The victim's content was unchanged in all 27,000 trials across both platforms** (Linux 2,783 / 2,834 /
**2,685** aliased across three runs; Windows 5 then 6). So this is *not*
CPE-1958's destruction bug, and CPE-1958's headline property — bytes can no longer reach a pre-existing
object — holds. What lands on disk instead is:

> a **successful-looking confirmed overwrite that did not write the user's bytes** and left the
> destination as a **second name for a file outside the scope root**.

The trade CPE-1958 makes, stated with the numbers rather than as an unqualified win: the confirmed path
swaps a *destruction* race (bytes lost outside the root, 356/2,000 Windows and 188/10,000 Linux against
the pre-fix body) for an *aliasing* race (bytes not written, destination aliased). That is the better
position — a file the user never named is no longer overwritten — but it is a trade, not a clean sweep.

## What it needs, and why it is its own ticket

The commit has to name the source by **handle**, not by path: `renameat(dirfd, tmp_name, dirfd, target)`
against a directory handle the operation already holds, so the entry that moves is the one this process
created rather than whatever now sits at that name.

**`std` does not expose `renameat`.** This crate already has the handle-relative primitive it would
build on — `batch_media::open_beneath`, added by CPE-1896 for the destination open, which reaches
`NtCreateFile` through the already-vendored `windows` crate on Windows and would need the `libc`/`rustix`
equivalent on Unix. **One `renameat` in `open_beneath` unblocks all three of these:**

- this ticket (`stage_and_replace_at`'s commit),
- **CPE-1961** (`claim_destination_handle` and `batch_media::open_output_verified`, which name the same
  missing primitive),
- `copilot::apply_op`, deferred for exactly this reason.

That is why it is filed rather than fixed inside CPE-1958: it is one shared primitive with three
consumers, and doing it inside a hard-link ticket would give it neither the design nor the review it
needs.

## Acceptance criteria

- [ ] **Re-measure first, with `cpe_1958_rename_source_report`, on both platforms.** Do not start from
      the fix. Keep the **delete-only control** — without it a changed number proves nothing.
- [ ] Add `renameat`/handle-relative rename to `batch_media::open_beneath` (or a sibling in the same
      module), with the Windows arm going through `NtSetInformationFile`'s `FileRenameInformationEx`
      against a directory handle, and the Unix arm through `renameat`.
- [ ] Route `stage_and_replace_at`'s commit through it. **Keep `ReplaceFileW` working** for
      `Commit::CarryingTheDestination` — the editor's save needs its carry-over, and that arm has its own
      (different) exposure to think about.
- [ ] **Assert on the filesystem** — the destination's identity, and the user's bytes actually present at
      the name — never on a verdict enum.
- [ ] Report before/after at comparable trial counts on **both** platforms. Windows' 6/3,000 and Linux's
      2,834/3,000 are the same defect at very different rates; a fix measured only on Windows proves
      almost nothing.
- [ ] While there: say whether the same primitive closes **CPE-1961**'s two sites, and if so, say so on
      that ticket rather than silently fixing them.

## Notes

Filed 2026-08-27 from PR #1070 round 2 (CPE-1958), Auditor finding F3, re-measured by that PR's worker.
The invariant's doc comment at `Commit::ReplacingTheName` has been corrected in that PR to say what it
actually covers, and `overwrite_confirmed_no_follow`'s doc carries a "What this does NOT close" section
pointing here — so this is recorded at the site, not only in the queue.

Family: **CPE-1958** (the destination race this one is the other half of), **CPE-1961** (the two live
check-then-use sites), **CPE-1896** (`open_beneath`, the primitive this would extend), **CPE-1739**
(where `stage_and_replace` came from), **CPE-1738** (the `.cpe-tmp` residue this makes enumerable).
