---
id: CPE-1846
title: restore can close the final-component link swap with the NOFOLLOW pattern the crate already ships
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-21
closed:
---

## Problem

CPE-1823 left one recorded residual: at the final path component, `confined_to` canonicalises and
**refuses** a planted link — the Security Auditor confirmed this with **17,488 successful symlink plants
across a live restore and zero writes through** — but the check and the subsequent `fs::copy` are not
atomic. The residual is the microseconds between them. The Auditor could not win it; it is real but narrow.

The relevant discovery is that **this crate already ships the structural fix.**

`crates/server/src/batch_media.rs:1587-1600` documents a four-step pattern. Step 2 is *never follow a
link at the final component* — `O_NOFOLLOW` on Unix, `FILE_FLAG_OPEN_REPARSE_POINT` on Windows —
hard-coded per target with **no `libc` dependency** (`:1679-1691`) and pinned by a runtime test.
`batch_execute.rs:583` already uses it.

Opening the restore target with that flag and writing through the handle, instead of `fs::copy`, closes
the final-component link swap **structurally** at both `snapshot_capture::restore` and
`revert_engine::apply_write` — without refusing a single legitimate overwrite.

## Why this is separate from CPE-1823

It changes `fs::copy`'s attribute-preserving behaviour on Windows, which is a real behavioural change
needing its own measurement and its own review. CPE-1823 correctly declined `copy_file_into_claimed_slot`
(that helper uses `create_new`, which refuses an existing name — and restore-over-a-tree and
`revert_engine`'s first-class `Overwrite` both depend on writing onto one). But that rejection covered
only **step 1** of the crate's four-step pattern. Step 2 is the half restore can actually use, because
opening an existing regular file for truncate-and-write is exactly what overwrite means.

## Acceptance criteria

- [ ] `restore` and `apply_write` open the final component with the no-follow flag and write through the
      handle. Reuse `batch_media.rs`'s existing per-target implementation — do not write a second one, and
      do not add a `libc` dependency.
- [ ] Measure and record what changes about attribute preservation on Windows versus `fs::copy`
      (timestamps, ADS, attributes, sparseness). If anything regresses, say so and decide explicitly.
- [ ] A legitimate overwrite of an existing regular file still succeeds — restore-over-a-tree and
      `Overwrite` both. This is the constraint that killed the `create_new` approach; do not reintroduce it.
- [ ] Re-run the Auditor's final-component race (a racer replacing the target with a symlink throughout a
      multi-thousand-entry restore) and report plants attempted versus writes through.
- [ ] The interior-component race remains the recorded residual either way. Say so plainly rather than
      implying the class is fully closed.
- [ ] Red-proof each new test with the minimal realistic change, observe red, revert, record the line.

## Notes

Found by the independent Reviewer during CPE-1823's round-4 review. Its exact objection is worth keeping:
the CPE-1823 code comment should not describe the residual as irreducible when `batch_execute.rs:583`
already reduces it. That wording is being corrected in CPE-1823's round 5; this ticket is the actual work.

Read CPE-1823's final Work Log first — it carries the attack record, including why `canonicalize` cannot
see a hard link and the rule that a guard belongs where callers inherit it rather than at each call site.

## Work Log

### 2026-08-22 — the residual was winnable, and the crate's own pattern closes it

**The headline: the residual CPE-1823 recorded as "real but narrow, and the auditor could not win it"
is winnable, and this ticket's own racer won it — twice.** CPE-1823's auditor raced *blind*: it planted
symlinks continuously from the start of the restore, which means it planted one before pass 1 had
finished resolving, `confined_to` refused, the abort was total and **no write ever happened**. Measured
here on exactly that shape: 12/12 rounds refused, **0 entries written**, 23,340 live plants, 0 writes
through — a clean-looking zero that proves nothing about a window the run never entered.

Triggering the racer on the **first restored byte** instead (CPE-1823 round 4's own insight: the first
byte on disk is the signal pass 1 is over) puts the plants inside the write window. Against unmodified
`fs::copy`:

```text
                        rounds  live plants  entries written  WRITES THROUGH
fs::copy   run 1          40       4,989          2,012            1   <- victim104.txt = "RESTORED CONTENT 104"
fs::copy   run 2          40       5,000          ~2,000           0
fs::copy   run 3          40       4,969          ~2,000           0
fs::copy   run 4          40       4,981          ~2,000           1   <- victim104.txt = "RESTORED CONTENT 104"
                        totals    19,939                           2 escapes in 4 runs
```

An escape is a manifest entry's bytes landing on a file **outside the restore folder**, with the restore
having reported nothing unusual. Roughly one escape per ~10,000 live plants, and one run in two. So the
residual was not theoretical; it was under-measured because the attack had been aimed slightly wrong.

With `fsutil::copy_file_onto_no_follow` in place, same harness, same machine:

```text
                        rounds  live plants  entries written  WRITES THROUGH
no-follow  run 1          40       5,369          1,542            0
no-follow  run 2          40       5,159          1,443            0
no-follow  run 3          40       4,959          1,351            0
no-follow  run 4          40       5,022          1,342            0
no-follow  run 5          40       5,041          1,385            0
                        totals    25,550          7,063            0
```

Link shape verified rather than inferred, per the CPE-1844 lesson: the harness probes its own plant
before measuring anything and prints what Windows says it made — `fsutil reparsepoint query` →
**`Reparse Tag Value : 0xa000000c`**, a real symlink (not `0xa0000003`, a junction). Developer Mode is on
here, so `symlink_file` succeeds where `New-Item -ItemType SymbolicLink` would refuse; nothing was
inferred from a cmdlet failing.

**A harness bug caught before it became a finding.** The first triggered run reported **2,681 writes
through** — all of them the harness's own fault. Its per-round setup wrote `PRE-EXISTING` into every
destination name with `fs::write`, which *follows* a link the racer had already planted there, poisoning
2,681 victims before the production code ran at all. Fixed by disarming the racer during setup and
staging with `create_new`. Recorded because "2,681 escapes" would have been a spectacular false finding.

**The fix, reusing the crate's existing per-target implementation and adding no `libc` dependency.**
`batch_media::open_no_follow` is now `pub(crate)` and is the *only* place `O_NOFOLLOW` /
`FILE_FLAG_OPEN_REPARSE_POINT` are spelled — its constants are already pinned by
`secaudit_open_output_verified_refuses_a_symlink_final_component`, and a second copy could have drifted
out from under that test silently. `fsutil::copy_file_onto_no_follow(src, dst)` wraps it: open without
following, refuse the handle if it addresses a link or a directory, `set_len(0)`, stream via the existing
`stream_bytes`, carry permissions and (Windows) mtime via the existing `carry_file_times`. Both sinks
call it — `snapshot_capture::restore` pass 2 and `revert_engine::apply_write`.

Step 1 of `batch_media`'s four-step pattern (`create_new`, claim the name) is still **not** taken, for
CPE-1823's reason: it refuses an existing name, and restore-over-a-tree and `Overwrite` both need one.

### Attribute preservation versus `fs::copy` — measured on Windows/NTFS, not assumed

Source carried an ADS (`Zone.Identifier`, `ZoneId=3`), mtime epoch 1,000,000,000, creation time epoch
900,000,000, and separately the read-only attribute and a 16 MiB sparse region. Copied both ways into a
fresh name **and** onto an existing file that carried its own `Zone.Identifier` (`ZoneId=9`):

| property | `fs::copy` | `copy_file_onto_no_follow` | verdict |
|---|---|---|---|
| content bytes | identical | identical | same |
| modification time | `1000000000` carried | `1000000000` carried | same |
| creation time | stamped "now" | stamped "now" | same — neither carries it |
| read-only attribute | carried | carried | same |
| `FILE_ATTRIBUTE_SPARSE_FILE` | not carried | not carried | same — neither carries it |
| ADS onto a **fresh** name | `ZoneId=3` carried | **absent** | **REGRESSION** |
| ADS onto an **existing** file | `ZoneId=3` (destination's replaced) | `ZoneId=9` (destination's kept) | **REGRESSION, different shape** |
| plain source over an ADS destination | destination's stream **removed** | destination's stream **kept** | **REGRESSION, different shape** |

**Decided explicitly rather than discovered later.** Only the alternate-data-stream rows move. The last
two rows are the surprising half and are why they are tabulated separately: `CopyFileExW` *replaces* the
destination file, so its streams go with it, while a truncate-and-write leaves them attached to a file
object it did not replace. A restore over an existing file therefore does not merely lose the
checkpoint's Mark-of-the-Web — it leaves the **live file's** Mark-of-the-Web sitting over restored
content.

Accepted, for the reason `copy_file_into_claimed_slot` already accepted it and documents at length:
carrying a stream means addressing `dst:StreamName`, which is a **path**, so the carry re-opens the
destination by path after the handle was pinned — reintroducing exactly the window this ticket closes,
with a window as wide as the whole copy rather than a syscall (the PR #968 round-2 auditor won that race
on its first attempt). The direction of the error also favours safety: on the overwrite path an existing
warning is *kept*, not dropped. Written into `src/docs/16-checkpoints.md` so it reaches the user, not
only the code.

Unix is unaffected on every row. Creation mode is deliberately unchanged: `open_no_follow` creates at
`0666 & ~umask` and `set_permissions` narrows afterwards — the same order `std::fs::copy` itself uses, so
the brief window where a restored `0600` file sits at the default is exactly as wide as it was before.
Narrowing it needs a mode-taking `open_no_follow`, i.e. a second spelling of the constants, which is the
thing this reuse exists to avoid.

### Which guard does what — measured by sabotage, and it is not the obvious answer

Three things refuse a link here, and they are **not** interchangeable:

- **The no-follow open** is the structural half. Swapped for an ordinary `create(true)` open, the
  *dangling*-link test reds on its **harm** axis — the damage is done by the open itself, which
  materialises the link's target before any check can run.
- **The post-open refusals** are not redundant with it. With both disabled and the no-follow open intact,
  the live-link tests red on their **verdict** axis with `Ok(16)` while their **harm** axis stays green:
  on Windows a reparse-point handle accepts a truncate-and-write that reaches nothing, so without these a
  restore would report **success having written the bytes nowhere** — the silent-skip class this crate
  refuses (CPE-1803/1804/1805/1816).
- **Neither post-open check reds alone on Windows**, stated plainly rather than left as an un-pinned
  guard. They overlap only there. On Unix `O_NOFOLLOW` makes the `open` fail with `ELOOP` and neither
  runs; the path check exists for the case `batch_media`'s runtime test guards — a wrong hard-coded
  constant, which would leave it as the only defence.

### The interior-component race is NOT closed and remains the recorded residual

`safe_target` resolves the directories above the final component **by path**, and the open is by path
too, so a directory link swapped into an interior component between them still redirects the write.
Closing it needs `openat`-relative resolution, which `std` does not expose. Written next to the mechanism
in all three places (`fsutil`, `snapshot_capture::restore`, `revert_engine::apply_write`) rather than
implied away. A **hard link** at the destination is likewise still written through, exactly as before —
there is nothing to refuse on the handle and `canonicalize` cannot see it either; refusing multiply-linked
destinations (what `open_output_verified` does) would refuse legitimate overwrites of ordinary hard-linked
files, which is the constraint that killed `create_new` arriving by another route.

### Red-proofs — one line each, observed red, then reverted

| Test | Line broken | Observed |
|---|---|---|
| `cpe_1846_a_dangling_link_at_the_destination_is_never_created_through` | `crate::batch_media::open_no_follow(dst)` → `std::fs::OpenOptions::new().write(true).create(true).open(dst).map(\|f\| (f, false))` | `HARM: the write followed a dangling link and created its target at …\restore-target.txt-target-that-does-not-exist`. The live-link tests stayed **green** under this same line — the path check still refused them — which is why that is recorded above rather than presented as one guard doing all the work |
| `cpe_1846_a_live_file_link_at_the_destination_is_never_written_through` | the two post-open refusals disabled (`if false && std::fs::symlink_metadata(dst)…` **and** `let why = if false && facts.is_reparse_point`) | `writing onto a link at the final component must be refused: Ok(16)` — reds on the verdict, harm axis green. Neither line reds alone on Windows; both were tried individually first and both stayed green |
| `cpe_1846_restore_over_a_tree_overwrites_but_never_through_a_link_at_the_final_component` | `crate::fsutil::copy_file_onto_no_follow(&blob, &target)` → `fs::copy(&blob, &target)` in `snapshot_capture::restore` | `HARM: the restore wrote a manifest entry's bytes through a link at the final component, onto a file the manifest never named — restore returned Ok(())` |
| `cpe_1846_a_link_planted_at_an_overwrite_target_is_refused_and_the_other_overwrite_still_applies` | the same one line in `revert_engine::apply_write` | `HARM: the revert wrote the checkpoint's bytes through a link at the final component, onto a file the plan never named — report was RestoreReport { applied: 2, skipped: [] }` |
| `cpe_1846_a_legitimate_overwrite_of_an_existing_regular_file_still_succeeds` | `crate::batch_media::open_no_follow(dst)` → `claim_file_slot_with_mode(dst, birth_mode_of(&meta)).map(\|f\| (f, true))` (i.e. reintroducing the `create_new` approach CPE-1823 rejected) | all three legitimate-overwrite paths red at once: the unit pin, `…: was free when this operation picked the name and is not free now`; the revert's ordinary `Overwrite` (`applied: 0`, both entries skipped); and restore-over-a-tree, which could not even complete its clean run |

**Fixture liveness is folded into the helpers, not repeated per test** — the CPE-1844 fix. `make_file_link`
asserts the slot holds a link *and* that it resolves to the intended target, and `require_staged` turns a
staging failure on a platform that supports the mechanism into a **red**, not a silent skip;
`make_dangling_link` asserts the link exists and dangles. Each test then adds one cheap second proof by
*following* the link and asserting it reads the victim's bytes, so a link pointing somewhere harmless
cannot certify anything. The race harness carries the same check inside `plant_link`, which is why 6 of
37,100 plants were not counted as plants.

### Gates

`crates/server`: clippy `--all-targets -- -D warnings` → **exit 0**. `cargo test` → **2348 lib**
(4 ignored) + `archive_panic_safety` 21 + `binary_data_preview_panic_safety` 22 + `checkpoint_roundtrip` 2
+ `finder_tags_os_interop` 1 + `native_meta_os_interop` 1 + `parser_panic_safety` 45 + `sample_fixtures` 16
+ `thumb_svg_panic_safety` 32 + `ticket_mcp` 0, **0 failed**. Delta: **+5 lib tests**, exactly the five
added here (2343 → 2348); every other binary unchanged.

`src-tauri`, both feature modes: clippy default → **0**, clippy `--features sidecar-platform` → **0**;
`cargo test` → **214**, `--features sidecar-platform` → **269**. Delta from this change: **0** — no
`src-tauri` file is touched. (Those figures are +4 on CPE-1823's 210/265; the difference is tickets merged
between, not this one.) No `specta::Type` struct and no command signature changed, so `bindings.gen.ts` is
unaffected.

**Not verified locally:** every `#[cfg(unix)]` path through the new code, and in particular the claim that
`O_NOFOLLOW` makes the open itself fail with `ELOOP` so the post-open checks never run there. CI's ubuntu
and macOS `Server crates` legs are the only verification, and must be green on this head before merge. The
race numbers are Windows/NTFS only; the equivalent measurement on ext4/APFS has not been taken.

### CI — the merge gate, checked by SHA

PR **#1001**, head **`ea1cb2d3`** (first push). Every check on that SHA completed: **18 success, 1 skipped**
(`GUI smoke (windows-latest)`, conditional). The merge gate re-checked explicitly by SHA rather than by
branch — `gh pr checks --watch` exits 0 when the branch moves under it:

```text
Server crates (ubuntu-latest)  — clippy + test   completed  success  head_sha=ea1cb2d3…  run_id=97144961021
Server crates (macos-latest)   — clippy + test   completed  success  head_sha=ea1cb2d3…  run_id=97144960994
Server crates (windows-latest) — clippy + test   completed  success  head_sha=ea1cb2d3…  run_id=97144960983
```

**The Unix legs are confirmed live, not merely green.** Full job logs were downloaded (not read through
`gh run view --log`, which can return a silent prefix): **ubuntu 17,370 lines, macOS 16,079 lines**. Both
show all five `cpe_1846_*` tests running and passing, and — the part that matters — **zero**
`[CPE-1846] SKIPPED` notices on either, so the symlink fixtures really staged there and the tests asserted
rather than returning early.

### 2026-08-23 — independent Security Audit: MERGE, with two records this ticket owed

The same Security Auditor whose "17,488 plants, zero writes through" this ticket overturned returned
**MERGE** and confirmed the false negative for the stated reason. Two things it recorded are worse than my
own numbers, and one of them is a gap in this ticket that had to be closed before merge.

**It measured the SHIPPING sink, which I did not.** My race drove `snapshot_capture::restore`, which has
no production caller. It drove `execute_restore` — reached by `checkpoint_revert` /
`checkpoint_revert_one` — 10 rounds x 3,000 entries:

```text
                            live plants   applied   WRITES THROUGH
fs::copy baseline              352,373     29,959         63          <- EVERY round escaped
copy_file_onto_no_follow     1,029,661     29,999          0
```

Sample round: `applied=3000, skipped=0, escapes=4` — a revert reporting **complete success** while putting
four checkpoint payloads on files outside the reverted tree. That is arbitrary file write on the path
users actually reach, and far easier to hit than my ~1-per-10,000. The fix wrote **more** entries under
**3x** the plant density and produced **zero**, at a cost of one extra skip in 30,000.

**And it sharpened my account of the false negative, which was only half right.** I explained CPE-1823's
zero by its pre-pass abort — true for `snapshot_capture::restore`. But CPE-1823's log *also* records
"20,000+ blind swaps, zero escapes" against the **shipping** `execute_restore`, which has **no pre-pass
abort at all**. My mechanism does not explain that zero; it was simply under-powered aim. So **both zeros
were wrong, for two different reasons** — one structural (the abort fired before the window opened), one
just insufficient force. Recorded rather than left as one tidy explanation covering two different facts.

### The record this ticket owed: a 9x-29x wall-clock regression, never measured here

This ticket measured attribute preservation exhaustively and **never measured time**. Release build,
Windows/NTFS, no racers:

```text
shape                               fs::copy    no-follow    factor
3,000 x 19-byte files                1,485 ms   13,678 ms     9.2x    (0.5 ms -> 4.6 ms per file)
500 x 256 KiB  (128 MB total)          329 ms    9,628 ms      29x    (389 MB/s -> 13 MB/s)
```

A ~2 GB checkpoint revert goes from roughly **5 s to roughly 2.5 minutes**. Squarely against PURPOSE.md's
fast/small/predictable tiebreaker, and shipping it unrecorded was the one gap in an otherwise fully
measured ticket.

**The guards are not the cost.** Six variants were bisected: a bare `File::create` + `write_all` with
**zero** guards costs the same as the shipped fix. The whole regression is inherent to leaving
`CopyFileExW` for a user-mode handle write, consistent with Defender scanning each file on close — per-file
fixed cost, not per-byte.

**Why the crate's existing benchmark missed it, which is the transferable lesson.** `COPY_CHUNK`'s doc
benchmarks **one 200 MB file** at 2,101 MB/s (66% of `CopyFileExW`), amortising a single
open/close/scan over 200 MB. A restore is the opposite shape — **many files** — so that fixed cost is the
entire bill. **I reused that benchmark's conclusion for a shape it does not cover.** Do not cite
`COPY_CHUNK`'s figures for a many-file caller; measure the caller's own shape. Written into
`copy_file_onto_no_follow`'s doc next to the mechanism, with the numbers.

The security property is not traded against it: the baseline being replaced was measured putting 63
checkpoint payloads outside the reverted tree. A follow-up owns the speed (getting `CopyFileExW`
throughput without its resolve-the-destination-by-path semantics is real work, not a tweak). **Deliberately
NOT written into `src/docs/16-checkpoints.md`**: a user-facing "reverts are slow now" sentence would be
stale the moment that follow-up lands, and the docs already carry the two behaviour changes that are
durable (link refusal, Mark-of-the-Web).

### The one sentence that was wrong: the ADS door is closed, not locked

My reasoning said carrying a stream *means* addressing `dst:StreamName`, "and that is a path". True of a
`std`-only implementation — **but this crate already links `windows` 0.56** (it is how `handle_facts`
reads identity off a handle at all), so `BackupRead`/`BackupWrite` over the pinned handle, or a
handle-relative `NtCreateFile` with the stream name, would carry streams **without re-opening by path**
and without reopening the window. The decision stands on cost and scope — distinct Win32 stream-format
work, Windows-only, on a path whose bytes come from the user's own local store — but the framing told a
future reader the door was locked when it is only closed. Corrected in place.

**On the ADS trade the Auditor split my two shapes, and I agree with the split.** Rows 2 and 3 (a stale
Mark-of-the-Web kept over restored content) are **not** a security problem: the error points toward *more*
restriction, never less. **Row 1 is the one that weakens a control** — a `.docm` downloaded, captured and
later restored comes back with no Mark-of-the-Web, so no SmartScreen prompt and no Protected View. Modest
severity (the bytes are the user's own, from a local store), correctly traded against a demonstrated
arbitrary write, and stated in plain words in both this log and the user docs rather than buried.

### What it attacked, and what held

Every shape refused: a privilege-free **NTFS junction** (refused at the open; tag verified `0xa0000003`),
a real **directory**, a **symlink pointing at a bystander INSIDE the root** that `confined_to` correctly
admits — and the good one, a link planted **strictly after the open**, mid-copy of a 64 MiB blob:
`Ok(67108864)` with the victim untouched at 6 bytes. **Handle pinning proven: nothing after the open
re-opens by path.** The `create_new` red-proof held line by line, all three overwrite paths red with the
exact message. The guard-layering claim survived a four-way sabotage matrix exactly as written, including
both post-open checks off giving `Ok(16)` (the silent write-to-nowhere) and the following-open variant
materialising a dangling link's target **at the open**.

### Two smaller findings, both recorded in code

- **On Windows the `facts.is_dir` arm — and the reparse arm — appear unreachable**, because a directory
  or a junction fails the `open` itself, so those refusals surface as the generic "could not open the
  destination for writing", which omits the "Nothing was written for this entry" clause every other
  refusal carries. A real if minor wording gap. The arms are **kept**: unreachable is a per-platform fact,
  not a property of the code, and on a `handle_facts == None` platform the path check is the only other
  thing standing.
- **After a post-open swap the destination NAME holds the attacker's link while the bytes sit in the
  now-unlinked file object, and the call returns `Ok`.** Not an escape — it is handle pinning working —
  but the restored entry is not at the name the report implies. Detecting it would mean re-resolving the
  name after the write, i.e. asking a path question again at the one place this function exists to stop
  asking one.

### The harness is not in the tree, and that is a real gap in this PR

The PR is 6 files and none of them is a harness, so the Auditor **could not verify my self-inflict fix** —
only my account of it. (My first triggered run reported 2,681 escapes, all of them the harness writing its
own fixtures through links the racer had planted.) It built an independent harness that is *structurally
incapable* of that failure: racers only unlink and symlink, never write content, and every victim is
asserted byte-pristine immediately before the call as a hard panic. That is the better design, and mine
was not committed because it runs ~35 s, needs the symlink privilege, and wins on baseline only ~50% of
runs — a flaky pin, which is why the deterministic tests carry the property instead. Recorded as an
acknowledged gap: a committed harness built the Auditor's way (assert-pristine-then-call, racer never
writes) would be worth having, and belongs with the follow-up rather than in this merge.
