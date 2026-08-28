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

## Work Log — 2026-08-28

### Re-measured FIRST, on the merged state, before anything changed

`cpe_1958_rename_source_report`, Windows 11 / NTFS (local 4 TB volume, `TMP` pointed at it — checked
`DriveType 3`, so a real local disk, not a mapped or WSL mount), 3,000 trials per shape:

| shape | aliased | lying `Ok` | victim content changed |
|---|---|---|---|
| relink an outside victim | **8 / 3,000** | 8 / 3,000 | 0 |
| delete-only (CONTROL) | 0 / 3,000 | 0 / 3,000 | 0 |

**Round 2 correction: that single run was quoted as "the" figure and it is a rate.** Four further runs
of the same binary on the same volume, taken in round 2 against the actual merge base, gave **2, 3, 2, 1
aliased** (3, 3, 3, 1 lying), and an independent reviewer on another machine got **1, 1, 2, 4**. So the
Windows rate here is 1–8 per 3,000 and no single run settles it. A first 200-trial run read 9 / 200 and
was set aside as warm-up noise.

**Linux was NOT re-measured by this worker.** The local WSL distro has no C toolchain (`gcc`, `cc`,
`ld` all missing) and this crate needs one for `ring`, `zstd-sys`, `lzma-sys`, `bzip2-sys` and
`libsqlite3-sys`; installing it is offsite work this shift did not do. The Linux column above stands as
previously recorded and is **not** corroborated here.

### The fix

`stage_and_replace_at`'s `Commit::ReplacingTheName` arm now commits through
`open_beneath::rename_beneath` against a `RootDir` held on the destination's own folder — a new
`fsutil::StagedBeneath`. `Commit::CarryingTheDestination`, the editor's save, is untouched and still
commits with `ReplaceFileW`, carry-over intact.

Three things fell out that were not in the brief:

1. **The staging opener had to fork too.** `NtSetInformationFile(FileRenameInformation)` refuses a
   source handle without `DELETE`, which `std`'s `OpenOptions` does not request. The `Beneath` arm
   stages through `open_beneath::create_staging_beneath`, which asks for it; the `StagingCreateFails`
   injection seam is asked at the fork so it still covers both openers.
2. **`stage_bytes_over_checked_handle` had to take the destination handle BY VALUE and close it.**
   `std::fs::rename` tolerates a destination something holds open; `FileRenameInformation` does not.
   With the handle still lent, **17 tests** across `batch_execute`, `batch_media` and `archive` failed
   with "could not be replaced by the staged copy of it (Access is denied. (os error 5))" on ordinary,
   unattacked outputs.
3. **Unix cannot be fixed, only reported.** `renameat`'s source is a name and POSIX has no fd-sourced
   rename. So the commit is followed by an identity check,
   `StagedBeneath::landed_object_is_the_one_we_wrote`, which turns an aliased commit into an `Err`
   instead of an `Ok(())`. Windows is **prevented**; Unix is **reported**. Any one-line summary of this
   ticket that drops that split is wrong.

### After, same harness, same machine, 3,000 trials per shape

| shape | aliased | lying `Ok` | victim content changed |
|---|---|---|---|
| relink an outside victim | **0 / 3,000** (3 runs) | 0 (3 runs) | 0 |
| delete-only (CONTROL) | 0 / 3,000 (3 runs) | 0 (3 runs) | 0 |

### A NEW failure mode the control found, and two wrong intermediate claims

The handle-sourced commit introduced an outcome the by-path one did not have: an attacker unlinking the
staging file while this process holds it open leaves a disposition the NT rename does **not** clear, so
the rename succeeds, replaces the destination, and the object dies at the last handle close — nothing
at the name, `Ok(())` returned. The delete-only control, 0 / 3,000 on both platforms before this
ticket, is what surfaced it. Progression, all Windows / NTFS, lying `Ok`s per 3,000 trials:

| commit shape | relink | delete-only (CONTROL) |
|---|---|---|
| before (by-path `std::fs::rename`) | 8 | 0 |
| `rename_beneath`, verify with the staged handle still open | 3 | 1, 2, 3, 0 |
| `rename_beneath`, verify after `drop(staged)` only | 0 | 13, 11, 0, 0 |
| `rename_beneath`, verify after **both** handles close | 0, 0, 0 | 0, 0, 0 |

Row 3 is the one to read twice: dropping the staged handle first improved the relink shape and made the
control **worse**, because the verification then opened its own handle and kept the doomed object alive
across its own check. Two intermediate versions of the doc comment claimed a close that the next run
refuted; all four rows are recorded at the site rather than replaced by the last one.

### Deterministic test — and this is the Linux leg

`cpe_1963_relinking_the_staging_source_cannot_produce_a_successful_looking_overwrite` arms a new
`#[cfg(test)]` seam, `BETWEEN_STAGE_AND_COMMIT`, and performs the attack inside the window: one trial,
every runner, no racing. It asserts an **outcome set read off the filesystem** — what the destination
holds, whether it is the victim's inode, what the victim holds — never a verdict enum, and it carries a
positive control plus a "the attack actually staged" counter so it cannot pass vacuously. Windows takes
the "reported" branch; Linux is expected to take it too, for a different reason, and CI's Linux backend
job is what measures that.

Red-proof, run rather than asserted: with the commit forced back to `commit_replacement`, the test fails
with `CPE-1963: the commit returned Ok(()) while the destination is a second name for the outside
victim and holds Some("UNTOUCHED") instead of the user's bytes.` The seam sits **before** the arm fork,
because the first attempt put it inside `StagedBeneath::commit` and the same sabotage then failed on
`relinked == 0` — a broken-fixture message rather than the defect returning.

### CPE-1929 sabotage pair, run on WINDOWS

| sabotage | `cargo test -p cpe-server --lib` |
|---|---|
| baseline | 2,457 passed / 0 failed |
| disable the identity check (`if false { … } Ok(())`) | 2,457 passed / 0 failed |
| force its predicate to lie (`if true \|\| landed != written`) | 2,435 passed / **22 failed** |

One green, not two, so it is **not** a shadowed guard: the path is reached on every confirmed overwrite,
and on Windows its answer is always "same object" because `rename_beneath`'s source there is the staged
handle. On Unix it is the decider. **The pair was run on Windows only** — that the first sabotage would
red on Linux is a prediction, not a result. The `symlink_metadata`-after-drop half is invisible to the
suite either way; the racer table above is what measures it.

### A false claim removed (CPE-1933)

`open_beneath::rename_beneath`'s doc asserted that *"`fsutil::ClaimedDestination::commit` turns it into
a loud refusal by comparing the identity at the destination against the identity it wrote."* It does
not — `commit` syncs, renames, sweeps and returns `Ok`; the identity it captures is a public field whose
own doc says exactly one of five legs reads it. Corrected at the site, pointing at the comparison that
does now exist and saying which arm it is *not* on.

### Does the same primitive close `copilot::apply_op`? **No** — checked, not assumed

`rename_beneath` requires both operands to be **siblings under one held root handle**, and it is one
call in a five-armed match: `Rename` fits its shape, `Move` is cross-directory and refused by that
precondition outright, `Copy` and `Mkdir` need `create_beneath` / `create_dir_beneath`, and `Delete`
goes through an OS trash API that takes a **path** and has no handle-relative form on any platform. What
that site is waiting on is the *descent* — a `RootDir` held for the run with every arm re-expressed
against it — which is a change to `copilot`, not a primitive it lacks. Left deferred, and the reason is
now recorded at `copilot::apply_op` rather than pointing at a gap that has since been filled elsewhere.

### Still open after this ticket, named rather than implied

`ClaimedDestination::commit`'s `ByPath` arm (`revert_engine::apply_write`, `snapshot_capture::restore`)
still commits by path exactly as this one used to, and its `Beneath` arm carries the Unix residual with
no comparison in front of it — CPE-1961 measured 2,785 / 3,000 aliased there on Linux. Neither was
touched here.

### Checks

- `cargo test -p cpe-server` — 2,457 lib tests plus every integration target green, 0 failed.
- `cargo clippy --locked --all-targets -- -D warnings`, plain and `--features index` — clean.
- `npm test` — 19 failed / 5,316 passed. **Round 2 corrected the rationale, not the figure** — see below.
- In-app docs updated: `src/docs/organizing-macros.md`, `src/docs/explorer-batch-media.md`.

## Work Log — round 2 (2026-08-28), review of PR #1098

`SEC PASS` / `CHANGES REQUESTED`. Seven findings, all addressed. The mechanism was independently
confirmed; the two majors were a functional regression round 1 shipped and a guard that passed by a
different mechanism than it documented.

### MAJOR-1 — a third-party handle on the output failed the write

Round 1 moved the commit to `NtSetInformationFile(FileRenameInformation)`, **the non-`Ex` form**, which
cannot replace a destination that has *any* handle open on it. Round 1 met that as its own handle (the
17 test failures) and closed ours — fixing the symptom and leaving the cause. On Windows a third-party
handle on a file being saved is routine: Defender, the Search indexer, Explorer preview and thumbnail
handlers, OneDrive, media players. It hit both `ReplacingTheName` callers, including **Batch Media**,
whose outputs are exactly the media files a thumbnailer holds open.

Measured on an ordinary unattacked confirmed overwrite with one extra `std::fs::File::open` — the
friendliest share mode Windows has:

| build | result | bytes at the name |
|---|---|---|
| merge base `dd097e64` | `Ok(())` | the new ones |
| round-1 head `63dc04a5` | `Err` — Access is denied. (os error 5) | the **old** ones |
| round 2 | `Ok(())` | the new ones |

Fixed by asking for **`FileRenameInformationEx` with `FILE_RENAME_REPLACE_IF_EXISTS |
FILE_RENAME_POSIX_SEMANTICS`**. The source operand is still the staged handle, so the security property
is untouched. Pinned by a new test,
`cpe_1963_a_third_party_handle_on_the_destination_does_not_fail_an_ordinary_overwrite`, which asserts on
the bytes at the name.

**It fixes both consumers**, since `ClaimedDestination::commit`'s `Beneath` arm (backup, archive,
transfer — merged in #1089) shares `sys::rename`. Post-fix racer: **0 aliased / 0 lying `Ok` / 0 victim
changed per 3,000, three runs**, with the new counter reporting **20,880–24,246 attacks actually landing
on a staging file** in the relink shape and 2,986–3,000 in the control.

**What the `Ex` form costs, checked rather than assumed.** `FILE_RENAME_INFO`'s `Flags` member is
documented as Windows 10 **version 1607** and later, and POSIX-semantics rename is an **NTFS** feature —
FAT32, exFAT and several network redirectors refuse it outright. That matters because `sys::rename` is
on the **backup** path, and a backup destination is exactly where an exFAT USB drive or an SMB share
turns up; `Ex`-only would trade a rare third-party-handle failure for *every* write failing there. So
the plain form is kept as a fallback, its residual is stated at the site, and — because no such volume
exists on this machine — the branch is reached two ways: a shipped test
(`cpe_1963_the_rename_fallback_still_commits_when_the_ex_form_is_refused`, arming a new
`FORCE_RENAME_WITHOUT_EX` seam) and a sabotage forcing **every** `Ex` attempt to fail, which leaves the
lib suite at **2,459 passed / 1 failed** against a 2,460 / 0 baseline — the one failure being the
third-party-handle test, which is exactly the price of the fallback and nothing else.

### MAJOR-2 — the flagship guard passed by a different mechanism than it documented

Round 1's doc mapped *prevented → Windows* / *reported → Unix*. On Windows the shipped fixture took
**reported**, because the attacker's `unlink` sets a delete disposition on the staged object and the
rename refuses before the operand question is reached. So it proved *"a delete-pending handle cannot be
renamed"* — CPE-1929's reads-as-coverage shape, in this file's flagship guard.

A second case now covers the handle-source property. **Three spellings were tried and the first two
collapse into the same delete-disposition path, measured rather than reasoned:** `unlink(tmp)`;
`rename(aside, tmp)` — which also fails, because `MoveFileEx` with `MOVEFILE_REPLACE_EXISTING` sets a
delete disposition on the file it *replaces*; and `rename(tmp, stolen)` then `link(victim, tmp)`, which
moves the staged object to a **free** name and marks nothing. Only the third reaches the property:

| attack shape | Windows outcome | why |
|---|---|---|
| `UnlinkThenRelink` | reported | staged object delete-pending; the rename refuses |
| `MoveAsideThenRelink` | **prevented** | nothing marked, so the HANDLE decides — the property |

Both are kept, both red-proof against the by-path commit (`2 passed; 2 failed`, same message), and the
doc's platform attribution is corrected.

### MINOR-3 — "this mode is NEW" was refuted by my own control

Re-measured on the real merge base: **four runs, delete-only control 0 / 3,000 each (0 / 12,000)** here,
against the reviewer's **2 / 12,000** on the same pre-fix revision. So the delete-on-close lying `Ok`
exists before this change at a rate low enough that 12,000 trials can miss it. Corrected to "much rarer
before, not absent", with both machines' runs written into the table.

### MINOR-4 — the commit-failure message

`.map_err(|r| r.why)` passed `open_beneath`'s refusal through raw — leading with the folder and trailing
`[staged as "…cpe-tmp"]`, the exact defect CPE-1958 F2 fixed and which the staging-*create* failure
thirty lines above already honours. Now wrapped with `display_path(target)`. It is reachable without any
attack, on the `Ex`-refused fallback path.

### MINOR-5 — two cross-references this PR made stale

`open_beneath.rs`'s *"the only production caller"* (`StagedBeneath::commit` is now a second) and
`fsutil.rs`'s `ByPath` *"same commit `stage_and_replace_at` uses"* (now only its
`CarryingTheDestination` arm). Both corrected in place.

### MINOR-6 — ticket moved back to `Backlog/`

Not merged is not Done. The Foreman moves tickets at merge.

### NIT-7 — the racer now counts its own landed attacks

`cpe_1958_rename_source_race` reports `{n} attacks actually landed on a staging file` and prints an
explicit WARNING when that is zero, so a post-fix `0 / 3,000` cannot be read as a fix when it was really
a run that measured nothing.

### CPE-1929 pair, re-run on WINDOWS at the round-2 baseline

| sabotage | `cargo test -p cpe-server --lib` |
|---|---|
| baseline | 2,460 passed / 0 failed |
| disable the identity check | 2,460 passed / 0 failed |
| force its predicate to lie | 2,435 passed / **25 failed** |

Unchanged in shape from round 1: one green, not two, so not a shadowed guard. Windows only; the Linux
half remains a prediction.

### The `npm test` figure — corrected, with the cause

The reviewer measured **0 failed / 5,376 passed / 2 skipped** and could not reproduce mine. Re-measured
here three consecutive times on the round-2 head: **19 failed / 5,316 passed**, and the same command
with round 2 stashed gives **exactly the same 19 / 5,316** — so it is not this change. One run under
heavy load gave 44 failed / 5,325 passed / 9 skipped over the identical four files; the three settled
runs did not.

**Round 1's rationale was "pre-existing, untouched", which is true but explains nothing. The cause is
now measured:** the shell scripts under test exit **127** (`expected 127 to be 3`, `to be 4`, …) because
**`jq` is absent on this machine** — confirmed on both the PowerShell and Git-Bash search paths.
`catalogPublishLoudFailure.test.ts` self-skips on `!hasJq`; `catalogPublishVersion.test.ts` and
`catalogPublishFreshnessGuard.test.ts` have no such guard and fail instead. `releaseVerifyWiringGuard`'s
3 further failures were not characterised. Nothing here is attributable to this change, and the figure
is a property of this machine's toolchain rather than of the repo.

### Checks (round 2)

- `cargo test -p cpe-server` — **2,460** lib tests plus every integration target, 0 failed.
- `cargo clippy --locked --all-targets -- -D warnings`, plain and `--features index` — clean.
