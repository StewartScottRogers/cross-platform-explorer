---
id: CPE-1786
title: temp_extract_target leaks 1.4 million directories in production code — the real source CPE-1693 did not touch
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-19
closed:
---

## Problem

`crates/server/src/archive.rs::temp_extract_target` (around line 749) creates
`%TEMP%\cpe-archive\<pid>-<seq>\` for every archive extraction and **never removes it**. Its own doc
comment states this outright: *"Nothing here cleans up... this change adds one more directory per
extraction just as before."*

Measured on this machine during PR #934's UAT, 2026-08-19:

- `%TEMP%\cpe-archive\` contains **1,394,403** subdirectories.
- One isolated, 8-way-parallel run of just the archive test module added **12** more.

That is essentially the entire "1.29 million directory" crisis figure from CPE-1693's own Work Log,
**alive today and still growing**, in production code.

## Why this was missed, twice

CPE-1693 fixed the leak at the **test-helper** level — `scratch()` now returns a `Drop` guard, and a
measured 532 leaked directories per full `crates/server` test run became 2. That work is real and
correct. But it is a different leak from this one, and two things hid the difference:

1. **The counting method has a blind spot.** CPE-1693 counted **top-level** `%TEMP%\cpe-*` directories.
   `temp_extract_target` creates its directories *inside* one pre-existing top-level directory
   (`cpe-archive`), so 1.39 million of them register as **one** entry. Before/after counts across a full
   test run showed +1, which looked like the leak was closed. Counting one level deeper shows +12 from a
   single module.
2. **PR #934's Work Log dismissed it as "already guarded."** That conflates two different properties:
   CPE-1733 gave this function an exclusive `create_dir` to defend against **squatting/redirect**; it
   says nothing about **leak-freedom**. Being safe against a hostile pre-existing directory is not the
   same as removing your own afterwards.

## Why it matters — this is the site of both originally-reported failures

CPE-1693 was escalated to High because the backlog started manufacturing false test failures. Both were
here, not in the test helpers:

- `zip_lists_real_tree_and_extracts_inner_file` failed on a **PID collision** — so many
  `%TEMP%/cpe-archive/<pid>-<seq>` directories exist that a reused process id finds its scratch name
  already taken.
- `could not claim a private extraction directory ... after 1024 attempts` — `temp_extract_target`'s
  retry loop exhausting its **entire** budget, because 1024 consecutive candidate names were all taken.

CPE-1745's Done record already says this explicitly: *"CPE-1693 tracks the leak; not touched by this
ticket."* Nothing has touched it since. Both failures passed on rerun, which is the property that
teaches whoever sees them to press rerun instead of reading.

Note also that this is **user-facing**, not merely a test annoyance: every archive a real user extracts
leaves a directory behind in their `%TEMP%` forever.

## What to do

This is production code with a live consumer, so it cannot simply take the test helper's `Drop` guard —
the extracted content must outlive the function that creates it. The lifetime question is the whole
ticket:

- Establish who actually owns an extraction directory and for how long — is it the extraction call, the
  preview/transfer that consumes it, or the app session?
- Give it that lifetime explicitly. Options worth weighing: tie the directory to the consuming
  operation's lifetime and remove it on completion; or keep a session-scoped root cleaned on startup
  and shutdown; or a generation/age sweep on app start. Sweeping on startup is the cheapest thing that
  bounds the growth, but it does not fix a long-running session.
- Whatever the shape, **the PID/sequence namespace must stop being able to exhaust**. 1024 consecutive
  taken names is a symptom of unbounded accumulation, not of an unlucky PID.
- **Prove it red first**, per the Evidence Rules in `Ticketing/wiki.md`: measure
  `%TEMP%\cpe-archive`'s subdirectory count before and after an extraction run — one level down, not
  top-level — and show the growth stopping. Do not reuse CPE-1693's top-level counting method; it is
  precisely what hid this.
- Clear the existing 1.39 million as a one-shot, reusing the junction-safe purge from CPE-1693's PR
  #934 (verified there against a canary behind a nested junction).

## Notes

Filed by the Foreman from PR #934's independent UAT, 2026-08-19. The UAT could not force a live
1024-attempt exhaustion on demand — it needs a specific PID-reuse-into-an-occupied-range condition — so
the crash itself was not reproduced this time; the root cause was measured directly instead and is
fully intact.

Related: **CPE-1693** (the test-helper half of this leak, genuinely fixed), **CPE-1733** (the
squatting/redirect guard that was mistaken for a leak fix), **CPE-1745** (whose Done record already
recorded the gap), **CPE-1782** (sftp/ftp/net helper leaks).

## Work Log

### The ownership question, answered from the call sites rather than guessed

The test-helper fix (a `Drop` guard) genuinely cannot be reused here: the whole point of the extracted
file is that it **outlives** the call that made it. So the question is who owns it instead. There are
exactly three consumers, and all three were read:

1. **Open-in-external** (`App.svelte`) hands the path to `open_external` — an arbitrary OS application.
   Whether it ever opened, and when it is finished, is not observable from the backend.
2. **Drag-out** (`FileList.svelte`, alt-drag) stages *every* selected entry in a loop and only then hands
   the batch to the native drag; the OS copies the files at drop time, long after the staging function
   returned. The feature's own research note already concluded *"do NOT delete on Dropped — the OS copy
   may still be reading; session/periodic cleanup"*.
3. **Archive preview** (`src/lib/archivePreview.ts`) caches the path per (archive, entry) for the whole
   session — and, decisively, **re-validates the cached path before every reuse and re-extracts if it is
   gone**, because "the temp file can be reaped mid-session".

So the extraction call cannot own the directory (the path escapes it) and neither can the consuming
operation (it ends inside another process). **The session owns it.** Consumer 3 is already written to
survive reaping, which is the property that makes any owner other than "forever" possible at all.

### What shipped

- **One session root per process**, `%TEMP%/cpe-archive/s<pid>-<random>`, claimed with an exclusive
  `fs::create_dir`. The random half comes from `std::collections::hash_map::RandomState` — OS-seeded, in
  `std`, **no new dependency**.
- **Extractions are numbered inside it** (`e<seq>`). This is what stops the namespace exhausting: a
  monotonic counter inside a directory this process created moments ago and that nothing else numbers
  into cannot collide with itself. The 1024-attempt bound stays, but it now only guards a *deliberate*
  squatter, not accumulation. (`row1_a_squatted_temp_directory_is_stepped_over_not_written_into` was
  restaged inside the session root so the CPE-1733 guard is still genuinely exercised rather than being
  staged somewhere the extraction no longer looks.)
- **Cross-session reclamation**: claiming the session root also sweeps the shared root for session
  directories nothing has touched in 24 hours — including the pre-CPE-1786 `<pid>-<seq>` shape, so the
  old leak drains through the same mechanism. **The 1.39 million backlog was cleared by the one-shot
  purge below, not by this sweep**: 32 removals per launch against 1.39 M is ~43,500 launches, which is
  arithmetically true and practically not. The sweep's job is to stop the *re*-accumulation.
- **The one-shot purge is done.** The real `%TEMP%\cpe-archive` was renamed aside first — an instant
  same-volume move, so the live window was milliseconds rather than the hours the delete took, and no
  concurrently running test or app had the tree pulled out from under it mid-extraction — and then
  deleted with `rd /s /q`, detached at idle priority. `rd`, deliberately **not** `robocopy /MIR`: PR
  #934 measured `/MIR` following a junction out of `%TEMP%` and deleting the far side *even with* `/XJ`.
  The root now holds 3 entries (verified by the Reviewer), all of them this round's own verification-run
  session roots — exactly the residue CPE-1797's shutdown hook exists to clear. Budgeted (256 examined / 32 removed per launch) and
  **synchronous**, because a detached sweeper thread is killed when a short-lived process exits and would
  reliably never finish. Liveness is the session root's own mtime, which creating each `e<seq>` child
  updates for free.
- **In-session reclamation**: past 512 live extraction directories the oldest are removed, but only once
  the process has been **quiet** for 10 minutes. See the corrections below — the first version of this asked the
  wrong question and introduced a data-loss path.
- `archive::cleanup_extraction_scratch()` removes the whole session tree for an embedder that knows the
  session is over. **It has no call sites** — `src-tauri/src/lib.rs` was owned by another worker this
  shift — so today every session is reclaimed by the next launch's sweep or by the cap, never at
  shutdown. Wiring it is **CPE-1797**.
- Failures throughout are **skipped, not reported**, which is deliberately `CLAUDE.md`'s `list_dir`
  philosophy: this runs on the first extraction of a session, so a cleanup that turned somebody else's
  unreadable litter into an error would fail the user's extraction. A useful side effect on Windows: a
  file another application still holds open cannot be deleted, so it protects itself for free.

### Evidence — and the counting method, which is the trap

Per the Evidence Rules. **Not** CPE-1693's top-level count: `cpe-archive` is a single top-level entry, so
1.39 million directories inside it register as **one**. Every count below is taken **one level down**, in
a **fresh empty synthetic temp root** (`TMP`/`TEMP` redirected into `crates/server/target/`), never the
real `%TEMP%` — a walk of that one passed 200,000 children without finishing.

Method: reset the synthetic root, then run the same filtered test binary
(`cpe_server-*.exe extract_archive_entry --test-threads=8`, 5 tests, 11 extractions) four times, counting
after each run.

**Red — before the change:**

```
run 1 -> top-level cpe-* = 1 | children of cpe-archive = 11
run 2 -> top-level cpe-* = 1 | children of cpe-archive = 22
run 3 -> top-level cpe-* = 1 | children of cpe-archive = 33
run 4 -> top-level cpe-* = 1 | children of cpe-archive = 44
```

The top-level column is the blind spot, live: it reads `1` while 44 directories accumulate one level
down. +11 per run, one per extraction, forever.

**Green — after the change, default one-hour TTL** (so nothing is old enough to sweep yet):

```
run 1 -> top-level cpe-* = 1 | children of cpe-archive = 1 | grandchildren = 11
run 2 -> top-level cpe-* = 1 | children of cpe-archive = 2 | grandchildren = 22
run 3 -> top-level cpe-* = 1 | children of cpe-archive = 3 | grandchildren = 33
run 4 -> top-level cpe-* = 1 | children of cpe-archive = 4 | grandchildren = 44
```

The unit of growth changed from *per extraction* to *per process*, and — the point — it is now a single
directory that can be reclaimed as a whole.

**Green — with `CPE_ARCHIVE_TEMP_TTL_SECS=0`**, which is exactly what a session older than the TTL looks
like to the next launch:

```
   diag: [CPE-1786] swept 0 stale session(s) under <TMP>\cpe-archive (examined at most 256, removal budget 32, ttl 0ns)
run 1 -> top-level cpe-* = 1 | children of cpe-archive = 1 | grandchildren = 11
   diag: [CPE-1786] swept 1 stale session(s) under <TMP>\cpe-archive (examined at most 256, removal budget 32, ttl 0ns)
run 2 -> top-level cpe-* = 1 | children of cpe-archive = 1 | grandchildren = 11
   diag: [CPE-1786] swept 1 stale session(s) under <TMP>\cpe-archive (examined at most 256, removal budget 32, ttl 0ns)
run 3 -> top-level cpe-* = 1 | children of cpe-archive = 1 | grandchildren = 11
   diag: [CPE-1786] swept 1 stale session(s) under <TMP>\cpe-archive (examined at most 256, removal budget 32, ttl 0ns)
run 4 -> top-level cpe-* = 1 | children of cpe-archive = 1 | grandchildren = 11
```

**Growth stopped.** Flat at every depth across four runs, where the same measurement before the change
read 11 / 22 / 33 / 44 — and the sweep's own diagnostic line (added this round) corroborates the count
from the inside rather than leaving it to be inferred from the directory listing.

*Harness note, recorded because it briefly looked like a real defect:* an early version of this loop
piped the child's stderr through `Select-String … | Select-Object -First 1`, which closes the pipeline
and **kills the test process mid-run**. That produced a plausible-looking anomaly — grandchildren falling
11 / 6 / 4 / 3 and top-level scratch directories accumulating, because a killed process runs no `Drop`.
The numbers above are from a `Start-Process -Wait` harness that redirects to a file instead.

### Gates

`cargo clippy --all-targets -- -D warnings` clean in all three CI feature modes for `crates/server`
(default; `index`; `pdf-thumb,video-thumb,waveform,dicom-thumb`). Full `cargo test` green (2,229 lib +
all integration binaries). No new dependencies. No `specta::Type` struct touched, so no bindings
regeneration.

Clippy also caught the correction round's one real portability bug before CI did: `Option::is_none_or`
is stable since 1.82 and this crate's MSRV is **1.77.2**, so `incompatible_msrv` failed the build. Now
`map_or`.

### Correction round — PR #945 review + independent UAT

The first push shipped a **new silent data-loss path that did not exist before**, because nothing was
ever deleted before. Recording it in full, since the mistake was not in the code but in believing a
comment the code did not implement.

`drain_reapable` asked *"is the **oldest** entry older than the grace?"* and its comment claimed that
"covers" the alt-drag staging loop. It does not. The queue is capped, so during a batch the front sits
`max_live` entries behind the entry being pushed, and the front's age is `max_live × per-entry staging
time` — **not** time since the batch started. `extract_archive_entry_any` reopens the archive and streams
a fresh decoder from byte zero for every entry (O(n²)), so per-entry time on a large `.tar.gz`/`.7z` is
seconds. The Reviewer derived the threshold arithmetically (`64 × per_entry ≥ 60 s`); the UAT, which had
not seen that analysis, reproduced it from the outside with a 90-entry loop at ~1 s per entry:

```
MID-LOOP-LOSS at step 64: earlier batch indices [0] vanished while this batch is still being staged
FINAL missing-of-90: [0, 1, 2, ..., 25]
```

26 of 90 files deleted before the batch had finished staging, let alone before the OS saw the drop —
`Ok` returned before the reap, and the reap is `let _ =`, so no error anywhere.

**Fixed by gating on *quiet* instead of on any individual directory's age**: reclaim only once no
extraction has *started* in the last 60 s, decided **before** pushing the new entry, with a
`HARD_CAP_EXTRACTIONS` of 4096 so a never-quiet caller stays bounded. That makes a burst atomically safe
however long it runs and however slow each entry is. `MAX_LIVE_EXTRACTIONS` was also raised 64 → 512, to
bound the shape the quiet gate cannot see (a gap longer than the grace is indistinguishable from an idle
session). **The residual figure written here in round 2 was wrong and is corrected in round 3 below** —
it needs *one* long gap, not uniformly slow entries.

**The test that should have caught it was the fifth "can only ever pass" shape this crew found today**:
it built all 200 batch entries with the *identical* timestamp, so the batch was instantaneous and no
time-based reap could ever fire — the assertion held for every possible `max_live` and `grace`, including
the broken implementation it was guarding. Replaced with the UAT's shape: 100 entries one second apart,
spanning more than the grace, newest pushed just now. **Red-proved by restoring the old rule**:

```
panicked at src\archive.rs:3730:
36 directories were reclaimed from a drag-out that has not been handed to the OS yet.
test result: FAILED. 0 passed; 1 failed
```

and green with the quiet gate. A second test pins the hard cap.

Also from that round:

- **`SESSION_TTL` 1 h → 24 h.** Liveness is mtime alone with no PID check, so a short TTL makes sweeping
  a *live* session reachable: instance A extracts at 10:00 and idles, instance B launches at 11:05 and
  removes A's session root. The recovery arm restores the directory but not the files, so an external
  editor's open temp copy loses the user's Save. An hour is a realistic afternoon; a day is not, and the
  drain rate is set by the removal budget per launch, not by the TTL.
- **A `session.lock` sentinel was added and then removed again** — see the third round below. It did
  nothing.
- **`cleanup_extraction_scratch` was worse than a no-op in degraded mode**: it `clear()`ed the queue
  before `remove_session_tree` (correctly) refused the shared root, so shutdown removed nothing *and*
  destroyed the only record of the `e<seq>` directories under it. Now drains and removes them first, and
  the body is split into a testable `cleanup_session(session, recorded)` so the degraded shape has a test
  that does not mutate process-global state.
- **The Windows open-handle protection was overstated.** The UAT measured it: a genuinely locked file (a
  `FileStream` denying delete) *is* protected and the sweep skips it cleanly, but a modern `notepad.exe`
  reads and releases its handle immediately and the file was deleted out from under an open window. The
  comment now says which class of consumer it covers instead of implying a blanket guarantee.
- **The sweep's result is now logged** (`examined/removed/ttl`) when the diagnostic env var is set, via a
  direct `writeln!` to stderr so libtest cannot swallow it — silence on the extraction path is right, but
  a sweep that quietly never works would have looked identical to one that does.
- **The examine budget takes directory order**, so an unremovable entry permanently occupies a slot in
  the window; documented, because a reader would otherwise compute the drain rate and be wrong.
- **A claim of mine was wrong and is corrected**: I wrote that CPE-1733's squat test, left where it was,
  "would have been a test that could only ever pass". The Reviewer diffed the old body — it would have
  **panicked** on the `strip_prefix('e')` parse, i.e. gone red. The restaging is still correct and
  necessary; the characterisation was not.

Confirmed unchanged by the UAT through the real production path: the fast path (65 rapid extractions all
survive; after a real 61-second wait the two oldest go and 64 survive), the junction defence (a real
Windows junction with a session-shaped name pointing outside `%TEMP%`, swept at `TTL=0`, canary
untouched), and the before/after counts reproduced to the digit by a third party.

### Third round — two comments that asserted a hazard away

Both blockers were the **same shape as round 1's**, which is the finding worth keeping: not wrong code,
but a comment claiming a protection nobody had measured. The rule adopted for the rest of the file was
*measure it or soften it to what you measured*.

**1. The declared residual was wrong by three orders of magnitude.** The comment said the residual needed
"more than 512 entries **each** taking over a minute — eight hours of staging". That is a *sufficient*
condition presented as a *necessary* one. `quiet` reads only `live.back()`, so **one** inter-entry gap
over the grace opens the gate for that single push however fast everything else was — and the O(n²)
re-decode this same file documents means the long gaps arrive exactly when the queue is longest. The
re-reviewer measured it on the shipped code:

```
601-entry alt-drag: entries 0..599 at 100 ms each, entry 600 takes 61 s
PROBE: elapsed since batch start = 121s; reclaimed = 89; batch left = 512; first due = Some("f0")
```

89 directories reclaimed out from under a still-staging drag, **two minutes in, not eight hours**.

- The claim is now stated as the true **necessary** condition: more than `max_live` live entries **and**
  any single inter-arrival gap of at least the grace.
- The real mitigation is a boundary move: **`REAP_GRACE` 1 minute → 10 minutes**, since the gap length is
  the entire exposure. A single archive entry must now take over ten minutes, mid-batch, in a batch
  already past 512. It costs only that an idle session tidies itself ten minutes later; the bound is
  `HARD_CAP_EXTRACTIONS`, not the grace.
- **The residual is now a test, not a sentence.** `cpe_1786_the_quiet_gate_protects_a_slow_batch_but_one_long_gap_is_the_known_residual`
  pins both halves — a 601-entry batch spanning minutes with every gap under the grace is untouched, and
  the one-long-gap probe still loses the overflow (`reclaimed = 89`, reproduced exactly). If someone
  later closes the hole, that assertion tells them so.

**The prescribed two-line fix was measured and deliberately not taken.** The prescription was: when
`quiet`, also require the popped entry's own age ≥ grace. Timestamps are taken under the queue's own lock
immediately before the push, so the queue is monotonic and `front` is never newer than `back`; whenever
`quiet` holds (`now - back ≥ grace`), `now - front ≥ now - back ≥ grace` already holds for every entry.
Verified by compiling the extra condition in and re-running the probe — **identical output, still 89
reclaimed**. Shipping it would have added a condition that can never fire: the code-shaped version of the
identical-timestamp test this ticket was already caught by. The invariant it rests on is pinned by
`cpe_1786_the_live_queue_is_monotonic_so_the_front_is_never_newer_than_the_back`, so if timestamps ever
stop being monotonic the prescription becomes live again and that test says so.

**2. The session sentinel provided zero protection, measured.** The claim was that holding a file open
inside the session root makes `remove_dir_all` fail so another instance's sweep bounces. The re-reviewer
built two binaries and measured it cross-process:

```
[cross-process] remove_dir_all = Ok(())
[cross-process] session still exists = false
[cross-process] e0/a.txt still exists = false
```

`fs::File::create` uses Rust's default Windows share mode (`READ|WRITE|DELETE`) and std's
`remove_dir_all` uses POSIX-semantics deletes, so the entire live tree went, files included.
`share_mode(1)` does make the root survive — but the same measurement showed the **contents are deleted
first**, so even the repaired version would not save the files it existed to protect. **The mechanism was
removed rather than repaired**, and `SESSION_TTL` now says outright that it is the whole protection. A
mechanism whose honest description is "the empty directory survives" is not worth the code, and an
overstated protection is worse than none because it stops the next person checking.

The irony is recorded in the code: this file already documented that an *external* application's handle
protects only one class of consumer and not `notepad.exe` — and then made exactly that assumption about
its own handle.

**Other claims audited in the same pass**, per the Foreman's instruction to measure or soften every
remaining one:

- *"creating each `e<seq>` subdirectory updates the mtime … on every platform this ships to"* — now
  **measured on Windows** (parent `LastWriteTimeUtc` `05:38:18.544` → `05:38:19.794` on creating one
  child) and explicitly marked **unmeasured** on Linux/macOS rather than claimed for all three.
- *"`remove_dir_all` would not delete through a symlink either"* — softened to "documented, not measured
  here", with the explicit skip kept as the thing actually relied on.
- *"a reader that already has the file open keeps reading it (Unix)"* — marked as asserted from POSIX
  semantics, not measured here.
- *"reading 256 of 1.39 million is cheap"* — reworded to what is actually true (`read_dir` yields
  lazily) now that the 1.39 M is gone.
- `RandomState` *"varies per call"* — the one claim that was cheap to **measure**, so it now is:
  `cpe_1786_session_names_vary_between_calls` draws 64 names and asserts all distinct. A fixed-seed
  hasher would make every process pick the same session name and turn the exclusive `create_dir` into a
  permanent collision.

### Platform caveat

Verified on Windows only. Two behaviours the Linux and macOS CI legs are the real check for: the
mtime-as-liveness signal (creating a child updates the parent directory's mtime — **measured here on
Windows**, ordinary POSIX behaviour elsewhere but not measured); and
`fs::symlink_metadata(..).file_type().is_symlink()` reporting `true` for a Windows junction, which is why
the sweeper's link leg could stage one at all. The link leg announces a loud `require_staged` skip on any
machine that cannot create a directory link, so a runner that stops being able to stage it goes red
rather than passing vacuously.

**A genuine cross-platform exposure that CI will not catch, flagged rather than fixed** (PR #945 review):
Linux `/tmp` is shared **between users**, where Windows `%TEMP%` is per-user. `cpe-archive` is created
`0755` by whichever user gets there first, so a *second* user cannot create a session root inside it,
falls into degraded mode, and then hits a hard `Err` from the per-extraction `create_dir`; and the
sweeper will attempt to remove another user's session root, where only the sticky bit saves it. **Both
are pre-existing** — the shared root and its permissions predate this ticket — so neither is introduced
here, but the session directory makes the first one easier to hit and it should be a ticket of its own.
