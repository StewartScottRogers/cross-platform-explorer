---
id: CPE-1765
title: The name-pick-to-write gap lets a copy land outside the folder the user chose (measured TOCTOU)
type: bug
priority: High
status: Backlog
tags: ready
estimate: L
created: 2026-08-17
closed:
---

## Problem

**Demonstrated**, not theorised, by the independent Security Auditor on PR #924 (CPE-1715) while auditing
that PR's symlink handling. It is **pre-existing** — CPE-1715 neither introduced nor worsened it, and that
PR correctly did not claim to fix it — but it is now measured, so it should stop being a comment.

Every name-picking write in the app is **probe-then-write, not atomic-create**:

- `do_copy_into` (`src-tauri/src/lib.rs:2576-2577`) and `do_move_into` (`:2649`, `:2662`) call
  `unique_target` to *pick* a free name, then write to that same `PathBuf` with a plain
  `fs::copy` / `fs::File::create` / `fs::rename` — no re-check, no `create_new(true)`, no lock.
- `run_transfer` (`:3282`) does the same via `resolve_conflict` + `fs::rename`, and its
  `copy_tree_streamed` / `stream_copy_file` path via `File::create`.

The code's own docs already flag this as unfixed — `crates/server/src/fsutil.rs:116-118` ("TOCTOU. Nothing
between this probe and the write is atomic… this is not a substitute for it") and `:1757` ("It is not
atomic with the primitive"). This ticket is the work those comments describe.

## The measurement

Planting a **live symlink at the picked name after the probe and before the write**:

**Copy-shaped write** (`fs::write` / `File::create` — `do_copy_into`, `stream_copy_file`): the write
**follows the link straight out of the destination folder**.

```
[SEC-AUDIT] evil_target now contains: Ok("USER CONTENT")
[SEC-AUDIT] target slot is still a symlink: Ok(true)
```

Content the user believed went to `dest\victim.txt` landed at `evil_dir\outside.txt` — **outside the folder
they chose**. The user chose the destination *folder*; anything landing outside it is the bug.

**Rename-shaped write** (`fs::rename` — `do_move_into`, `run_transfer`'s same-volume branch): does not
write through (rename does not follow the final component), but **silently destroys** the link and replaces
it, with no error:

```
[SEC-AUDIT] evil_target contents unchanged: Ok("PRE-EXISTING OUTSIDE FILE")
[SEC-AUDIT] target slot is now a symlink: Ok(false)
```

## Why High despite needing a race

It requires concurrent write access to the destination directory during the operation — but that is not an
exotic precondition for a file explorer. A shared drive, a sync client (OneDrive/Dropbox) rewriting a folder
mid-copy, an extraction into a watched directory, or any second process are all ordinary. And the failure
shape is the one this repo keeps paying for: **the operation reports success** while the bytes are somewhere
the user never chose.

## What to do

This is a class fix, not a one-liner — hence L. Sketch, not prescription:

- **Copy path:** create with `OpenOptions::new().write(true).create_new(true)` so the write is the
  existence check. `create_new` refuses to follow an existing final-component symlink and fails if the name
  appeared in the gap — turning the race into a loud, retryable error instead of an escape. Then the picked
  name and the created file are the same decision.
- **Move/rename path:** decide what `fs::rename` should do when the slot was taken in the gap. Note the
  platform split — Windows `MoveFileEx` without `MOVEFILE_REPLACE_EXISTING` fails if the target exists,
  POSIX `rename(2)` silently replaces. Both need stating and testing, per OS.
- **Do not** "fix" this by re-probing just before the write. That narrows the window and leaves the bug —
  and a narrower race is harder to test, not safer.
- Audit every sibling write site, not the three named here. `unique_target` / `resolve_conflict` /
  `probe_name_pick_slot` callers are the entry points.

## Acceptance criteria

- [x] A link planted at the picked name **between probe and write** cannot cause a copy to write outside the
      destination folder. Demonstrate with the auditor's reproduction, showing the new behaviour.
- [x] The move/rename case is decided, documented per-OS, and tested on all three — not left implicit.
- [x] The failure is **loud**: a name taken in the gap surfaces an error naming the path, never a silent
      success and never a silent clobber.
- [x] Every name-picking write site is covered or explicitly listed as out of scope with a reason. A fix at
      three sites that leaves four siblings is how this class keeps returning.
- [x] Each test asserts the **harm** (where the bytes actually landed, whether the link survived) **before**
      unwrapping the `Result` — every defect in this family fails by succeeding, so an assertion after an
      `unwrap` is unreachable exactly when it matters.
- [x] The two `fsutil.rs` doc comments (`:116-118`, `:1757`) that currently say this is unfixed are updated
      to describe what is now true.

## Notes

Found by the Security Auditor on **PR #924 / CPE-1715**, 2026-08-17, during the batched sprint. Related:
CPE-1715 (the dangling-link probe, which this sits underneath), CPE-1744, CPE-1758, CPE-1709 — the same
"reports success, bytes elsewhere" family.

## Work Log

### 2026-08-20 — implemented (branch `cpe-1765-name-pick-write-gap`)

**The decision: claim the name, don't re-check it.** New primitives in `crates/server/src/fsutil.rs`:
`slot_taken_message` (pure, path-naming), `claim_file_slot` (`create_new`), `claim_dir_slot`
(`create_dir`, **not** `create_dir_all`), `copy_file_into_claimed_slot`, `copy_tree_into_claimed_slot`,
`SlotClaim` (RAII placeholder) and `rename_into_claimed_slot` returning
`Renamed` / `SlotTaken(msg)` / `RenameFailed(io::Error)`. No new dependencies.

**Measured first, then designed** (Windows 11, stable `rustc`, standalone probe):

```text
rename(dir  -> existing FILE)        Ok      the file was replaced by a directory
rename(dir  -> existing EMPTY DIR)   Ok      contents moved in
rename(dir  -> existing NONEMPTY)    Err DirectoryNotEmpty
rename(file -> existing FILE)        Ok      bytes replaced
rename(file -> existing DIR)         Err PermissionDenied
rename(file -> junction)             Ok      LINK DESTROYED, no error   <- the defect
create_dir(junction)                 Err AlreadyExists (os error 183)
create_new(junction)                 Err PermissionDenied (os error 5)
create_new(file symlink)             Err AlreadyExists (os error 80)
fs::write(file symlink)              Ok  -> outside.txt now "USER CONTENT"  <- the escape
```

Rows 1–2 disproved the `MoveFileEx` folklore this design was nearly built on (modern `std` renames on
Windows with POSIX semantics), which is why **one** mechanism serves all three platforms instead of a
`cfg` split: the rename replaces this process's *own* placeholder — a file for a file source, an empty
directory for a directory source — on Windows and POSIX alike. Pinned per-OS by
`cpe_1765_a_free_name_still_moves_a_file_and_a_directory`, which CI runs on Linux, macOS and Windows.

**Sites.** Fixed: `do_copy_into`, `do_move_into` (both split so the write half —
`write_copy_into_picked_slot` / `write_move_into_picked_slot` — is callable from a test),
`copy_dir_all` incl. every interior name, `run_transfer`'s move fast path, `copy_tree_streamed`
(`create_dir_all` said `Ok` to a directory link — a whole tree left the folder that way),
`stream_copy_file`, and `snapshot_capture::save_manifest` (a sibling the ticket did not name:
`pick_manifest_id` proved an id free and `fs::write` then truncated whoever took it). Explicitly out of
scope with reasons — `batch_media` (own planner + own reparse-tag classifier; converting it changes the
transform pipeline's output contract), `backup::copy_one_verified` (derived mirror destination, not a
picked name), `save_store` (one fixed name), `stage_and_replace` (edits an existing name, already
atomic). The full table is in `fsutil.rs`'s CPE-1765 section header.

**Trade-off accepted and recorded:** the file copier streams through the claimed handle, so Windows
`CopyFileEx` extras (NTFS alternate data streams) are no longer carried. Permission bits still are. This
makes `do_copy_into` agree with `stream_copy_file`, which has always been a manual byte loop.

**Residual, stated rather than glossed:** a process that *deletes this process's placeholder* mid-move
could still have its replacement renamed over. It cannot send bytes outside the folder (rename does not
follow the final component) and needs an actor deleting files it did not create.

**Evidence.** 11 new `fsutil` tests + 5 in `src-tauri` + 2 in `snapshot_capture`, all asserting the harm
(where the bytes landed, whether the link/occupant survived) **before** touching the `Result`. Red-proofs
run and reverted: `claim_file_slot`→`File::create` reds 4; `claim_dir_slot`→`create_dir_all` reds 2;
`SlotClaim::drop` neutralised reds 1; reverting all four app write sites reds 4 (each printing
`result Ok(...)` — the defect's exact signature); `save_manifest`→`fs::write` reds 1; a copier that
refuses everything reds the parity tests, so an over-refusing "fix" cannot pass.

**Not verified locally:** the Linux and macOS legs. Everything above was measured and run on Windows;
the POSIX rows of the table are `rename(2)`'s specification, and CI's 3-OS backend matrix is what
actually exercises them. The live-file-symlink leg announces a loud skip on a Windows runner without
`SeCreateSymbolicLinkPrivilege`; a privilege-free hard-link leg asserts the same harm everywhere.

Docs: `src/docs/03-explorer.md` explains the new per-item message. No new Section, so `sectionDocs.ts`
is unchanged.

### 2026-08-20 — rework after review (Security Auditor + UAT + code Reviewer)

Three independent reviews of the first cut. The claim-then-write mechanism was confirmed sound (13
distinct attacks failed against it, and the Reviewer independently reproduced the Windows
`FileRenameInfoEx` POSIX-semantics finding by proving `fs::rename` replaced a destination it was
*holding open*, which `MoveFileExW` cannot do). Nine findings addressed:

**F1 — a HIGH regression this ticket introduced, found independently by two reviewers.**
`rename_into_claimed_slot` used `metadata(src).is_dir()`, which *follows* a link — so moving a directory
shortcut staked a directory placeholder, the rename failed, and the fallback dereferenced the link,
copied the linked-to tree into the destination and deleted the shortcut. Point it at `~/.ssh` and that is
where the keys go, with `Ok` returned. One word: `symlink_metadata`. Measured after:
`rename(junction -> our FILE placeholder) = Ok`, slot is still a link, linked-to tree untouched. **The
gap behind the gap:** all 18 original tests staged a link at the *destination* and none as the *source*.
Two tests added at each level.

**Copy arm — the Foreman's `COPY_FILE_FAIL_IF_EXISTS` route was tested and REJECTED on evidence.** It
refuses a regular file (80), a hard link (80), a live symlink (80) and a junction (5) — but on a
**dangling** symlink it returns `Ok` and creates the link's target *outside the folder*. It asks whether
the resolved target exists, exactly as `try_exists` does, so it is a kernel-side probe, not a claim, and
it fails open on the easiest link shape to plant. Root cause of the throughput regression was instead
`std::io::copy`'s 8 KiB buffer: 200 MB warm, best of 3 — `CopyFileExW` 3191 MB/s, `io::copy` 840 MB/s,
1 MiB 2101 MB/s, 4 MiB 2129 MB/s. Now a 1 MiB loop (~66% of the kernel path, up from 26%), with
Linux/Android deliberately keeping `std::io::copy` so `copy_file_range`/reflink is not lost. The residual
1.5× is a real trade against the airtight property and is documented as the Foreman's call, not accepted
silently.

**F2** — the residual is now stated **per site**: the single-file copier is absolute (writes through the
claimed handle); the tree copiers are not (they claim a *name* and re-resolve each child), and closing
that needs `openat`-relative resolution, which `std` has on no platform (`File::open` cannot even open a
directory on Windows — measured). Follow-up ticket, not a line edit. The `?` abort-on-first-error is
documented as load-bearing per the auditor's race harness.

**F3** — `SlotClaim::drop` no longer deletes by path unconditionally. It keeps the placeholder's handle
and removes only what it proves is still its own: exact `(dev, ino)` on Unix; regular-file + zero-length +
matching timestamps on Windows, where no *stable* handle-identity API exists. Every uncertainty leaves the
file alone. Measured that a held handle is **not** protection: `remove_file` while open = `Ok`, then
`create_new` at the same name = `Ok`.

**F4** — the copy destination is now born at the source's mode instead of `0666 & ~umask`, closing a
world-readable window that spanned the whole copy (the CPE-1739 shape). `set_permissions` now propagates
instead of `let _ =`. **F5** — the source is `stat`ed *before* it is opened and non-regular files are
refused, so a FIFO cannot hang the copy, a `/dev/urandom` link cannot fill the volume, and a directory
cannot leave a stray file at the claimed name.

**Review finding A** — Windows copies lost the modified time (`CopyFileExW` carries file times, a stream
does not); every pasted file was re-dated, reordering "sort by date modified". Now carried.
**UAT finding 2** — `Zone.Identifier` is an ADS, so copies silently lost the Mark-of-the-Web and
SmartScreen stopped firing. Now carried explicitly. Other ADS are still dropped (needs `FindFirstStreamW`)
and that limit is stated rather than glossed.
**Review finding C** — `resolve_conflict`'s Overwrite arm swallowed its removal failure, which this change
turned from "merge" into a refusal blaming a race that never happened. It now surfaces the real reason
and names the path.

Also: unwind test for the claim's drop; a comment recording that interior directory claims are covered by
shared code path rather than by assertion; both `clippy.toml` copies updated (they still pointed at
`rename_into_slot`); crash-litter and watcher-churn residuals documented.

**Still not verified locally:** Linux and macOS. F4's birth-mode test is `cfg(unix)` and has never run on
this machine — CI's 3-OS matrix owns it. Every link leg was confirmed to genuinely *run* here (no
`SKIPPED` lines in the test output), including the live-file-symlink one.

### 2026-08-20 — attempt 3: cut the accretion, close the cross-volume half

Three rounds each closed real findings and each introduced a smaller one *in the layer added to close
the last*. The core survived 13+ attacks untouched. So this round removes rather than patches.

**`carry_mark_of_the_web` DELETED, not guarded.** Three independent reasons, any one sufficient: it
re-opened the destination BY PATH (`dst:Zone.Identifier` is a path), violating `claim_file_slot`'s own
stated contract with a window spanning the whole copy — the auditor won that race first try, wrote
outside the chosen folder, stripped a victim's `ZoneId=3`→`ZoneId=0` and returned `Ok(272629760)` with a
0-byte symlink at the destination; it was silently defeated by a read-only source anyway, because
`set_permissions` ran before it and the ADS write hit ACCESS_DENIED into a `let _ =`; and it gave away,
two lines later, the never-re-open-by-path property the throughput cost was paid for. Guarding it was
rejected because the only available guard (`placeholder_is_still_ours`) is forgeable and TOCTOU precisely
on Windows, the only platform with alternate streams. The loss is now documented emphatically. **The
mtime carry was tested against the same rule and KEPT** — `carry_file_times` calls `dst.set_times()` on
the handle, never re-opening the path.

**Cross-volume F1 (Reviewer finding 1) — the half round 2 left open.** `RenameFailed(EXDEV)` fell into
`write_copy_into_picked_slot`, which branches on `src.is_dir()` (follows the link), so a shortcut moved to
another volume was dereferenced and deleted. The fix's own doc used "move it to a USB stick" as its
example — a USB stick is a different volume, so the scenario chosen to explain the severity was the one
still broken. Now **refused loudly** rather than recreated: recreating a link on a far volume means
choosing between `symlink_dir` (privilege), a junction (Windows, absolute-only) and `symlink`, plus
deciding what a relative target means under a new root — a new mechanism at the end of a chain whose
lesson is that new mechanisms are where the holes came from. The cross-volume tail is extracted as
`cross_volume_move_into_picked_slot` so the test drives production rather than a mock.

**Reviewer 3 — my tunnelling measurement was wrong, and it was load-bearing.** Re-measured 3/3:
`created_preserved=true modified_preserved=true mod_delta=Some(0ns)`. Tunnelling restores **both**
timestamps, and `FileTimesExt::set_created`/`set_modified` forge both trivially
(`forged: created matches=true modified matches=true len=0 is_file=true`). The honest bound is now
stated: the length and type checks do the real work; timestamps are a weak signal, not identity. It
refuses accidents, sweepers and non-empty files; it does not resist a deliberate local attacker.

**Also landed:** the file arm of `resolve_conflict`'s Overwrite now surfaces its removal failure
(Reviewer 4); the `SlotClaim` directory arm is pinned by a test that plants a junction at a staked name
(Reviewer 6 — deleting the guard previously left all 2264 tests green); both `clippy.toml` files fixed —
my round-2 edit had left them naming `rename_into_slot_claimed_slot`, which does not exist (Reviewer 5);
the pre-open stat is now the FIFO/device guard **only**, with `r.metadata()` from the open handle
authoritative for `is_file`, `birth_mode_of` and `set_permissions` (Auditor R2-F3 — strictly stronger
than both `fs::copy` and the previous round); and the auditor's `FILE_FLAG_OPEN_REPARSE_POINT` note is in
the doc so nobody simplifies `create_new` away.

**Finding C wording corrected after UAT.** UAT proved "nothing was replaced" overclaims: `remove_dir_all`
deletes as it walks, so a locked file leaves the folder holding *only* the locked file — the unlocked
siblings are already gone. Not a regression (the pre-fix code made the identical call and reported
success), but the message now says "nothing **new** was written" and warns the item may be partly
removed. Docs updated to state that Replace deletes before it writes.

**Performance, stated plainly including where it disappointed.** UAT confirmed the large-file fix
independently (77% of baseline, better than my 66% claim). It also found a small-file regime I had not
measured: ~79% of baseline on 2000×8KB, diagnosed as per-file syscall overhead including the MotW stream
read. **Deleting the MotW read did not recover it.** 2000×8KB, warm, best of 3, three separate runs —
main 692/723/810 ms, round 2 779/865/1167 ms, attempt 3 804/749/1031 ms. Attempt 3 beat round 2 in two
runs of three and lost in one; the variance swamps the effect. The residual small-file cost is the
per-file metadata syscalls (claim + `set_permissions` + `set_times` + the guard stat), not the stream
read and not the buffer. No improvement is claimed.

**Not attempted, deliberately:** the tree-copier containment partial (CPE-1825) and a `CreateFileW`
identity layer — both are new mechanisms, and this ticket's history says that is where the holes came
from.

**Still not verified locally:** Linux and macOS; the birth-mode test is `cfg(unix)` and has never run
here. The cross-volume test drives the real `EXDEV` tail directly rather than staging two volumes, so the
volume boundary itself is exercised by the Reviewer's C:→Z: measurement, not by CI.

### 2026-08-20 - CI went red on Linux, which is the point

Every round of this ticket carried the same caveat: *"Not verified locally: Linux and macOS. CI's 3-OS
matrix owns those legs."* It was not throat-clearing. `Server crates (ubuntu-latest)` failed:

```text
error: constant `COPY_CHUNK` is never used
error: could not compile `cpe-server` (lib) due to 1 previous error
error: could not compile `cpe-server` (lib test) due to 1 previous error
```

`COPY_CHUNK` is read only by the non-Linux arm of `stream_bytes` - Linux keeps `std::io::copy` for the
`copy_file_range`/reflink specialisation - so on Linux the constant is dead, and CI runs clippy with
`-D warnings`. Windows and macOS both take the loop arm, so nothing local could have caught it. Every
other job on the head was green: Backend on all three OSes, Frontend, Sidecar platform on all three,
Network E2E, GUI smoke, ffmpeg pin.

**Fixed by gating the constant with the loop's predicate, copied verbatim** -
`#[cfg(not(any(target_os = "linux", target_os = "android")))]` - verified byte-for-byte identical to the
one on `stream_bytes` so the two cannot drift. Both intra-doc `[`COPY_CHUNK`]` links were demoted to plain
code spans as well, since on Linux they would point at an item that no longer exists.

**Reproduced and red-proofed locally without a Linux box**, by temporarily swapping `target_os = "linux"`
for `target_os = "windows"` in both predicates so this machine took the `io::copy` arm exactly as Linux
does: clippy reproduced the CI error verbatim, the fix turned it green on that arm, and the predicates
were then restored. That is a repeatable technique for cfg-gated code on a single-platform workstation.

**Audited the rest of the PR for the same shape**, since a second dead item on macOS would cost another
full CI cycle. Every cfg-gated item added by this ticket defines **both** arms - `stream_bytes`,
`birth_mode_of`, `carry_file_times`, `same_object` - and every consumer of them is unconditional, so no
item can exist in only one platform's build. `COPY_CHUNK` was the only single-consumer item, and macOS
is just (loop arm) + (unix arms), a combination containing nothing that Windows and Linux do not each
already compile.

Lesson worth keeping: a cfg-gated helper is safe, but a cfg-gated *consumer* of an ungated item is not,
and `-D warnings` makes that a build failure rather than a warning.
