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
  directories nothing has touched in an hour — including the pre-CPE-1786 `<pid>-<seq>` shape, so the old
  leak drains through the same mechanism. Budgeted (256 examined / 32 removed per launch) and
  **synchronous**, because a detached sweeper thread is killed when a short-lived process exits and would
  reliably never finish. Liveness is the session root's own mtime, which creating each `e<seq>` child
  updates for free.
- **In-session reclamation**: past 64 live extraction directories the oldest are removed, but **nothing
  younger than 60 s is ever touched** — that grace is what protects a large drag-out batch that is still
  being staged. This is the half a startup-only sweep cannot provide.
- `archive::cleanup_extraction_scratch()` removes the whole session tree for an embedder that knows the
  session is over. **Not yet wired into the app's exit path** — `src-tauri/src/lib.rs` was owned by
  another worker this shift — so that wiring is the one follow-up this ticket leaves.
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
run 1 -> top-level cpe-* = 1 | children of cpe-archive = 1 | grandchildren = 11
run 2 -> top-level cpe-* = 1 | children of cpe-archive = 1 | grandchildren = 11
run 3 -> top-level cpe-* = 1 | children of cpe-archive = 1 | grandchildren = 11
run 4 -> top-level cpe-* = 1 | children of cpe-archive = 1 | grandchildren = 11
```

**Growth stopped.** Flat at every depth across four runs, where the same measurement before the change
read 11 / 22 / 33 / 44.

### Gates

`cargo clippy --all-targets -- -D warnings` clean in all three CI feature modes for `crates/server`
(default; `index`; `pdf-thumb,video-thumb,waveform,dicom-thumb`). Full `cargo test` green (2,224 lib +
all integration binaries), and `cargo test --features index --lib` green (2,272). No new dependencies.
No `specta::Type` struct touched, so no bindings regeneration.

### Platform caveat

Verified on Windows only. Two behaviours the Linux and macOS CI legs are the real check for: the
mtime-as-liveness signal (creating a child updates the parent directory's mtime — true on all three, but
only measured here), and `fs::symlink_metadata(..).file_type().is_symlink()` reporting `true` for a
Windows junction (which is why the sweeper's link leg staged one at all). The link leg announces a loud
skip via `require_staged` on any machine that cannot create a directory link, so a runner that stops
being able to stage it goes red rather than passing vacuously.
