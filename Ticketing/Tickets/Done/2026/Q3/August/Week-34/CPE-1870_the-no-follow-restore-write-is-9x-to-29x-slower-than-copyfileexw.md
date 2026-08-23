---
id: CPE-1870
title: the no-follow restore write is 9x-29x slower, and the benchmark that cleared it measured the wrong shape
type: task
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-23
closed: 2026-08-23
---

## Problem

CPE-1846 closed a demonstrated arbitrary-file-write by replacing `fs::copy` (`CopyFileExW`) with a
no-follow open plus a user-mode handle write. That was the right trade against 63 escapes per 10 rounds on
the shipping revert path. It also costs, and nobody measured it until the audit:

| workload | `fs::copy` | `copy_file_onto_no_follow` | factor |
|---|---|---|---|
| 3,000 x 19-byte files | 1,485 ms | 13,678 ms | **9.2x** |
| 500 x 256 KiB (128 MB) | 329 ms | 9,628 ms | **29x** (389 MB/s → 13 MB/s) |

A 2 GB checkpoint revert goes from about 5 seconds to about **2.5 minutes**. That is a user-visible
operation and it lands against PURPOSE.md's fast/small/predictable tiebreaker.

## The guards are not the cost

The audit bisected six variants rather than assuming:

```
as shipped                                        9640 / 9653 ms
BASELINE fs::copy                                  332 /  338 ms
set_len(0) removed                                9875 / 9660 ms
set_permissions removed                           9599 / 9708 ms
carry_file_times removed                          9553 / 9724 ms
both removed                                      9636 / 9598 ms
ALL guards gone: plain File::create + stream      9493 / 9514 ms   <- same as shipped
```

A bare `File::create` + `write_all` with **zero** guards costs the same. The entire regression is inherent
to leaving `CopyFileExW` for a user-mode handle write — consistent with Defender scanning each file on
close. So removing safety would buy nothing; the fix has to keep the handle-pinning property.

## Why the existing benchmark cleared it

`COPY_CHUNK`'s doc benchmarks **one 200 MB file** at 2,101 MB/s through a handle. That amortises a single
open/close/scan over 200 MB. **A restore is many files**, so it is all per-file fixed cost — 0.5 ms → 4.6 ms
per tiny file, 0.66 ms → 19.3 ms per 256 KiB file.

CPE-1846 reused that benchmark's conclusion for a shape it does not cover. That is the durable lesson here,
and it is worth carrying beyond this ticket: a throughput number from one large file says nothing about a
many-small-files workload.

## The shape of a fix

The property that must survive is **handle pinning** — the audit proved it by planting a link strictly
after the open, mid-copy of a 64 MiB blob, and getting `Ok(67108864)` with the victim untouched.

Candidate: `CopyFileExW` into a `create_new`-claimed sibling name in the same directory, then
`ReplaceFileW` or a rename onto the target. That keeps the kernel copy path and never writes through a
name an attacker can swap. It needs its own audit — a rename introduces its own window — but it is the
obvious place to start.

## Acceptance criteria

- [ ] Measure before changing anything, on both shapes: many small files and a few large ones. The 200 MB
      single-file benchmark is not evidence for either.
- [ ] Any fix keeps handle pinning. Re-run the audit's post-open plant (link planted after the open,
      mid-copy of a large blob) and report the result.
- [ ] Re-run the triggered race against both sinks and report live plants, entries written, and writes
      through. **Publish the denominator** — "N plants, 0 escapes" without the count of entries actually
      written is what produced two false negatives on CPE-1823.
- [ ] Say what happens to the ADS regression CPE-1846 accepted. A `CopyFileExW`-into-a-sibling route may
      carry streams again, which would close that too — check rather than assume.
- [ ] Red-proof every test with the minimal realistic change, observe red, revert, record the line.

## Notes

Found by the independent Security Auditor during CPE-1846, which it recommended merging: *"shipping a 29x
regression on a user-visible operation without it appearing in the record is the one gap in an otherwise
fully-measured ticket."* The numbers are now in CPE-1846's Work Log; this ticket owns the fix.

Read CPE-1846's Work Log first — it carries the guard layering, the ADS measurements and the reason
`create_new` cannot be used at this site (it refuses an existing name, and overwrite is the whole point).

Related: CPE-1846 (the security fix), CPE-1823 (the residual it closed).

## Work Log

### 2026-08-23 — the headline number did not reproduce, and the real cost was not where anyone looked

**The 9.2x/29x regression this ticket is named after could not be reproduced.** Re-measured before
changing anything, on the same machine, the same NTFS volume, the same release profile, with Defender
real-time protection **on** (checked: `RealTimeProtectionEnabled = True`). The `fs::copy` baselines
reproduce almost exactly — CPE-1846 recorded 1,485 ms for 3,000 x 19 B and I measure 1,215 ms — but the
figures for `copy_file_onto_no_follow` are out by an order of magnitude:

```text
shape                         CPE-1846 recorded        re-measured here
3,000 x 19 B    fs::copy         1,485 ms                 1,215 ms
3,000 x 19 B    no-follow       13,678 ms  (9.2x)         1,788 ms  (1.45x)
500 x 256 KiB   fs::copy           329 ms                   267 ms
500 x 256 KiB   no-follow        9,628 ms  (29x)            472 ms  (1.68x)
```

Tested against the obvious explanation and it is not that: a Defender path exclusion on the repo volume
would have made a user-mode write cheap here, so the identical benchmark was run a second time under
`%LOCALAPPDATA%\Temp` on the system volume. Same picture, no factor of ten. The likeliest remaining
cause is that the original was taken on a machine running a sprint's worth of concurrent builds — a
measurement of the machine rather than of the function — and it cannot be re-tested, because that load
no longer exists. Recorded rather than quietly dropped: **a number nobody can reproduce is a fact about
the measurement, and both readings are now in the code next to each other.**

**My own first benchmark was wrong too, in a way worth carrying.** It wrote the *same* payload into
every file. An antivirus caches its verdict by content hash, so 3,000 identical files are one scan and
2,999 cache hits — precisely the per-file cost under test. Every figure in this log uses **unique
pseudorandom content per file, regenerated for every pass**. Fixing that made the shipped code look
*worse*, which is how I know it was hiding something.

### What the cost actually was: a 1 MiB allocation per file, not `CopyFileExW`

CPE-1846's Auditor bisected six variants and concluded the regression was "inherent to leaving
`CopyFileExW` for a user-mode handle write". The first half of that is right — a bare `File::create` +
`write_all` with zero guards costs the same as the shipped fix, so the guards are not the cost, and I
re-confirmed that. The conclusion is wrong. `stream_bytes` allocated a **flat 1 MiB buffer on every
call**, so a 19-byte file paid in full for a buffer sized for a 200 MB one.

That is the same blind spot the ticket already identifies, one level deeper: `COPY_CHUNK`'s benchmark
moves **one 200 MB file**, which amortises a single open, a single close, a single scan **and a single
1 MiB allocation** over 200 MB. A restore is the opposite shape and pays all four per file.

The fix is `stream_bytes` taking a size hint — the source's length, read off the **open source handle**,
never a path stat — and sizing the buffer to `hint.clamp(1, COPY_CHUNK)`. `COPY_CHUNK` becomes a
ceiling instead of a size. Both callers already hold that metadata. **Not one line of the security
mechanism is touched**: same `open_no_follow`, same post-open refusals, same order, same messages.

Measured by `cpe_1870_copy_cost_by_shape`, which is **committed** so the next person re-takes the
measurement instead of arbitrating between two prose accounts of it:

```text
shape                      fs::copy    before (flat 1 MiB)     after (sized)
3,000 x 19 B   fresh        1,215 ms    1,788 ms  (1.45x)      1,272 ms  (1.05x)
3,000 x 19 B   overwrite    1,098 ms    1,665 ms  (1.46x)      1,166 ms  (1.06x)
500 x 256 KiB  fresh          267 ms      472 ms  (1.68x)        342 ms  (1.28x)
500 x 256 KiB  overwrite      245 ms      389 ms  (1.45x)        315 ms  (1.29x)
```

29% off the many-small-files shape, landing within ~5% of the `fs::copy` baseline it was measured
against. Above 1 MiB the clamp returns the constant and the loop is byte-for-byte what it was, so
nothing about the large-file case changes.

### The ticket's proposed fix was built, measured, and rejected — on two separate grounds

The suggested route was `CopyFileExW` into a `create_new`-claimed sibling, then `ReplaceFileW` or a
rename onto the target. It was built and attacked rather than reasoned about, and the probes are worth
keeping because two of them are counter-intuitive.

What held up:

- **A `rename` onto a symlink NAME replaces the link. It does not write through it.** Verified with a
  live fixture (the link read the victim's bytes first): `fs::rename` → `Ok(())`, victim still
  `"VICTIM"`, destination no longer a symlink. Onto a directory it is refused (`Access is denied`).
- **A held handle really can pin the sibling name on Windows**, which is what makes the staged copy
  safe rather than merely obscure. Opened `create_new` with `share_mode(FILE_SHARE_READ|WRITE)` — delete
  denied, rename denied, **and `fs::copy` into it still succeeds**. With `std`'s default share mode
  (which includes `FILE_SHARE_DELETE`) the name is deletable, i.e. swappable. With `FILE_SHARE_READ`
  alone, `CopyFileExW` cannot get in at all. Exactly one share mode works.
- **It would close the ADS regression completely.** All three of CPE-1846's rows return to `fs::copy`
  behaviour, including the surprising one: a plain source over an ADS-carrying destination *removes*
  the destination's stream. `ZoneId=3` carried onto a fresh name and onto an existing file.
- **It would also close the hard-link write-through CPE-1846 recorded as still open.** With a hard link
  at the destination pointing outside the tree: `fs::copy` and the shipped no-follow write both put
  `"RESTORED"` on the outside victim; the staged route left it `"OUTSIDE"`.

What killed it:

- **`rename` silently destroys the destination's ACL.** `icacls` before and after, with an explicit
  `BUILTIN\Guests:(R)` ACE on the destination: `fs::copy`, the shipped no-follow write and
  `ReplaceFileW` all preserve it; the rename route reduces the file to the parent directory's
  inherited set (`(I)` on every entry, the explicit ACE gone). A restore that quietly widens a
  deliberately restricted file's permissions is a worse defect than the one it was buying speed for.
- **`ReplaceFileW`, which preserves the ACL, costs 7.5 ms per small file** — 16x the shipped code and
  20x the fixed code. It also preserves creation time, refuses a symlink target outright
  (`0x800705B8`), and *merges* the destination's own streams back in, which diverges from `fs::copy`.
  All correct behaviour, at an unpayable price.
- Confining the staged route to **fresh names only** (nothing to preserve, so the ACL objection
  evaporates) still measured *slower* than the fixed in-place write on both small shapes, winning only
  above ~1 MiB.
- Both placement routes **fail on a destination another process holds open** without `FILE_SHARE_DELETE`
  (`Access is denied`), where the in-place write succeeds. That would need a fallback, i.e. two code
  paths and two behaviours, for a route that is already losing.

```text
route                                  3,000 x 19 B overwrite   what it costs
in-place, sized buffer (SHIPPED)          0.377 ms/file          the baseline the ratios are quoted against
in-place, flat 1 MiB (CPE-1846)           0.555 ms/file          —
sibling + CopyFileExW + rename            0.529 ms/file          destroys the destination's ACL
sibling + CopyFileExW + ReplaceFileW      7.476 ms/file          ~20x the shipped code (~13x the old)
```

The two `in-place` rows come from the committed `cpe_1870_copy_cost_by_shape` (1,132 ms and 1,665 ms
over 3,000 files). The two `sibling` rows come from a throwaway harness that is **not** in the tree,
whose in-place reading in the same run was 0.306 ms/file — so quoting the sibling ratios against the
committed 0.377 is, if anything, conservative. Both are named because an earlier draft of this table
mixed them and produced a "16x" that followed from neither.

**So the ADS regression stands — and it now stands on numbers rather than on reasoning.** CPE-1846
accepted it on an argument about re-opening by path; that argument was later corrected to "closed, not
locked". This ticket establishes what the door costs to open: either the destination's ACL or 16x. Both
recorded in `copy_file_onto_no_follow`'s doc so the next person does not re-derive it.

### Handle pinning, re-proved rather than inherited

`cpe_1870_a_link_planted_after_the_open_cannot_redirect_the_copy` (committed, `#[ignore]`d) plants
symlinks at the destination name *strictly after* the copier opened it, while a 64 MiB blob streams
through the handle, and arms on an observed effect — the destination's length passing 4 MiB, which can
only happen after the open and after the first write.

```text
118 plants landed after the open   copy returned Ok(67108864)   victim untouched at 6 bytes
```

It asserts `plants > 0` before it asserts safety, so a run where nothing was planted fails rather than
reporting a comfortable zero.

### The race, re-run against both sinks — with the denominator published AND asserted

CPE-1846's Work Log records that its harness was never committed and that the auditor could therefore
verify only its account of itself. That harness had also, on its first run, reported **2,681 escapes**
that were entirely its own doing — its setup wrote fixture content with `fs::write`, which *follows* a
link the racer had planted. The harness is now **in the tree**, built so that failure is structurally
impossible:

1. **The racer only unlinks and symlinks. It never writes a byte of content**, so no bytes the harness
   produced can be mistaken for an escape.
2. **Every victim is asserted byte-pristine immediately before the call**, and a victim already damaged
   at that point fails the run instead of being counted as a finding.
3. **Armed on an observed effect of the code under test — the first restored byte** — never on "start".
4. **A plant counts only if the name really holds a link naming the victim**, checked with `read_link`,
   never by reading *through* it (which races the write under test and could file a successful escape
   as "not a live plant").
5. **The denominators are asserted, not merely printed**: zero live plants or zero entries written is a
   FAILURE, not a zero. That is exactly the shape of both CPE-1823 false negatives.

**A design mistake I made and had to measure my way out of.** The first racer planted continuously and
left the links in place. Result: essentially every name became hostile, the restore refused ~99% of its
entries, and the run reported *"29,836 live plants, 0 escapes"* against **303 entries written**. A
ferocious-looking attack that mostly never entered the window it was aiming at — the same class of
false negative this ticket is supposed to be guarding against, arriving from the opposite direction.
Fixed by making the racer **flicker**: it withdraws its own link again (an unlink, never a write) so the
names stay writable, and only ever removes a name it has just confirmed holds its own link, so a
legitimately restored file is never deleted out from under the denominator.

Final, on the shipped tree:

```text
sink                              rounds   live plants   ENTRIES WRITTEN   WRITES THROUGH
execute_restore (shipping)          10        56,614         29,971              0
snapshot_capture::restore           10         1,986             13              0
```

**The positive control, because a zero without one is worth nothing.** Both call sites were reverted to
`fs::copy` — the exact CPE-1846 red-proof line — and the same harness re-run:

```text
sink                              rounds   live plants   ENTRIES WRITTEN   WRITES THROUGH
execute_restore, fs::copy           10        61,538         29,990              5   <- 4 of 10 rounds
```

`round 0: applied 2999 (skipped 1), writes through 1` — a revert reporting success while putting a
checkpoint payload on a file outside the reverted tree. So the harness demonstrably produces a positive
on this machine in this run shape, and the zero above means something. Sabotage reverted; the tree is
clean.

**What the harness canNOT do, stated plainly.** It could not produce a positive against
`snapshot_capture::restore`: even with `fs::copy` restored there, pass 2 re-judges each entry, refuses
the planted link and aborts, so the run wrote 177 entries and escaped 0. That sink's numbers above are
therefore **an absence of evidence, not evidence of absence** — its denominator is 13 entries across 10
rounds, because the abort usually fires before the trigger. The meaningful measurement is
`execute_restore`, which is also the only one with a production caller
(`checkpoint_revert` / `checkpoint_revert_one`).

### Red-proofs — one line each, observed red, then reverted

| Test | Line broken | Observed |
|---|---|---|
| `cpe_1870_a_size_hint_shorter_than_the_source_still_copies_every_byte` | the `loop` replaced by a single `read` + `write_all` + `return` (the "the buffer is the file's size, so one read is enough" bug the hint invites) | `assertion left == right failed: hint 1 reported the wrong byte count, left: 1`. The 3 MiB test reddened on the same line at `left: 1048576` |
| `cpe_1870_a_size_hint_shorter_than_the_source_still_copies_every_byte` (hint 0 leg) | `size_hint.clamp(1, COPY_CHUNK as u64)` → `size_hint.min(COPY_CHUNK as u64)` | `hint 0 reported the wrong byte count, left: 0` — a zero-length buffer makes `read` return `Ok(0)` and the loop reads that as end-of-file, so a source with a stale or unavailable length would copy **nothing** and report success |
| `cpe_1870_a_source_larger_than_the_buffer_ceiling_copies_every_byte` | same single-read line as above | `left: 1048576` against 3 MiB + 777 bytes |
| `cpe_1870_an_empty_source_truncates_an_existing_destination_to_nothing` | `w.set_len(0)` → `if false { … }` | `the old tail must be gone, not merely unread` with the previous content still on disk — pins the new sizing against the existing truncate rather than either alone |
| `cpe_1870_triggered_race_against_the_shipping_revert_sink` | `crate::fsutil::copy_file_onto_no_follow(&blob, &target)` → `std::fs::copy(&blob, &target)` in `apply_write` | `5` writes through against 29,990 entries written under 61,538 live plants — the harness's positive control, above |

Fixture liveness is folded into the helpers rather than repeated per test, per the CPE-1844 lesson:
`cpe1870_plant` returns false unless the name really holds a link naming the victim, and
`cpe1870_victims_intact` panics rather than counts when a victim is already damaged *before* the call.

### Gates

`crates/server`: clippy `--all-targets -- -D warnings` → **exit 0**. `cargo test` → **2,359 lib passed,
8 ignored**, plus `archive_panic_safety` 21, `binary_data_preview_panic_safety` 22,
`checkpoint_roundtrip` 2, `finder_tags_os_interop` 1, `native_meta_os_interop` 1, `parser_panic_safety`
45, `sample_fixtures` 16, `thumb_svg_panic_safety` 32, `ticket_mcp` 0 — **0 failed**. Delta from
`origin/main`'s 2,356 passed + 4 ignored: **+3 passing** (the three deterministic pins) and **+4
ignored** (the two race harnesses, the post-open plant, the benchmark). Every integration binary
unchanged.

`src-tauri`, both feature modes: clippy default → **0**, clippy `--features sidecar-platform` → **0**;
`cargo test` → **214**, `--features sidecar-platform` → **269**. Delta **0** — no `src-tauri` file is
touched. No `specta::Type` struct and no command signature changed, so `bindings.gen.ts` is unaffected.

**What the green Unix legs do and do not cover — corrected, because I stated both halves loosely.**

- **Linux proves nothing about the sized loop.** `stream_bytes`'s Linux/Android arm keeps
  `std::io::copy` and takes `_size_hint`, so there is no buffer for a bad hint to get wrong and
  `cpe_1870_a_size_hint_shorter_than_the_source_still_copies_every_byte` **passes trivially** there.
  A green `Server crates (ubuntu-latest)` is evidence the change compiles and breaks nothing on Linux —
  it is **not** evidence for the mechanism this ticket adds.
- **macOS is stronger than "not verified".** macOS takes the sized loop, and CI's
  `Server crates (macos-latest)` leg runs all three deterministic tests green. So the sizing's
  *correctness* is covered on a second platform; only its *timing* was never measured there.

The race numbers, the ACL measurement, the ADS measurement and every timing are **Windows/NTFS only** —
the equivalents on ext4/APFS have not been taken.

**Why the four `#[ignore]`s, precisely.** Not simply "they need the symlink privilege":
`cpe1870_verdict` **fails** on `live_plants == 0`, deliberately, so a run that raced nothing cannot
report a comfortable zero. A GitHub Windows runner without Developer Mode or admin cannot create a
symlink at all, so un-`#[ignore]`ing these would turn that job red for a reason unrelated to the code.
The assertion that makes the harness honest locally is the one that makes it unusable in CI. Two things
keep that tolerable: `clippy --all-targets` and `cargo test` **compile** them on all three OSes so they
cannot silently bit-rot, and the refusals they probe are pinned deterministically by the `cpe_1846_*`
tests, which do run everywhere.

**Also not verified:** `cargo clippy/test --features index` and
`--features pdf-thumb,video-thumb,waveform,dicom-thumb`, which CI's `Server crates` job also runs. The
change is feature-independent (one function signature in `fsutil`), so the default mode is
representative, but I did not run them and CI is the check.

### 2026-08-23 — independent Review: APPROVED, and it re-took both columns rather than reading mine

The Reviewer reproduced the whole measurement independently and **confirms the 9.2x/29x does not
reproduce**: on its machine CPE-1846's code measures **1.44x-1.57x across all eight cells**, matching my
1.45x-1.68x to within noise. It also independently reproduced, on its own hardware:

- **the same-content collapse** — 1.03x and 0.99x on the small shape with a uniform payload against
  1.44x/1.45x with unique content, i.e. a benchmark written the obvious way reports *no regression at
  all*;
- the **ACL destruction**, with its own before/after `icacls`;
- the **share-mode pinning** table and the **ADS** rows, including the counter-intuitive one where a
  plain source removes the destination's stream;
- **my own self-caught false negative, almost exactly**: disabling only the withdraw-the-link line gave
  it 29,827 plants against **219** entries written, where I recorded 29,836 / 303. Its phrase for it —
  *a ferocious-looking attack with a two-order-of-magnitude denominator collapse* — is better than mine
  and is worth keeping;
- and it **fired the positive control itself**: 6 writes through in 5 of 10 rounds, against my 5 in 4.
  So the shipped zero is meaningful rather than comfortable, on two machines.

**Four documentation defects it found, all in shipped source, all of which would have outlived the PR.
Every one is mine and every one is now fixed.**

1. **"7 ignored" is 8.** Main carries 4 and I add exactly 4, so my *delta* (+3 passing, +4 ignored) was
   right and only the absolute was wrong — which is worse, not better, because the next person computes
   their delta from it. Corrected above.
2. **`copy_file_onto_no_follow`'s rejected-routes table quoted 0.306 ms/file for the shipped code**,
   while my own headline table implies 0.389 and the Reviewer measured 0.376. The 0.306 came from the
   throwaway harness, not the committed one — two harnesses, one table, no label. The table now carries
   both `in-place` rows, names which harness each number came from, and says which baseline the ratios
   are quoted against.
3. **The "16x slower than doing nothing" that followed from no row of its own table.** 7.476 / 0.306 is
   24x and 7.476 / 0.377 is 20x; 16x was quoting a baseline that appeared nowhere. Now stated as ~20x
   against the shipped code and ~13x against the pre-change code, with both spelled out.
4. **`COPY_CHUNK`'s three-column table said it was "measured by the committed
   `cpe_1870_copy_cost_by_shape`".** The committed test measures `fs::copy` and whatever the crate
   currently does — the middle "flat 1 MiB" column requires reverting the clamp first. A reader
   following that instruction gets two of three columns and no way to tell which is missing. The doc now
   names the one-line revert needed to reproduce the middle column.

Plus a minor overstatement it was right to catch: the flicker comment claimed the racer "only ever
removes a name it has just confirmed holds its OWN link", where the guard is actually
`file_type().is_symlink()`. Sound in practice — nothing else in the test creates links — but the
comment now says what the code checks rather than what I meant.

**And one consequence of my own stated limit that I had left for the reader to work out.** On Linux
`stream_bytes` keeps `io::copy` and takes `_size_hint`, so
`cpe_1870_a_size_hint_shorter_than_the_source_still_copies_every_byte` **passes trivially there** — a
green ubuntu leg is not evidence for the sized loop, and writing "not verified off Windows" invited
exactly the opposite reading. The converse also needed correcting in my favour: macOS *does* take the
sized loop and *does* run all three deterministic tests green, so correctness is covered on a second
platform even though timing is not. Both halves are now written into the test's own doc comment, where
someone reading a green board will actually meet them.

It also gave the reason for the four `#[ignore]`s more precisely than I did — the binding constraint is
`cpe1870_verdict` failing on `live_plants == 0`, not the privilege as such, so a Windows runner without
Developer Mode would go red for a reason unrelated to the code — and named the two mitigations that make
it tolerable (both gate commands still *compile* the tests, and the `cpe_1846_*` pins run on all three
OSes). Written into the harness comment.

**Documentation only: no code path, no test, no assertion changed.** Re-gated after the corrections.
