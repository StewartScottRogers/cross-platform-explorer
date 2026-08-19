---
id: CPE-1693
title: The test suite has left 145,000 directories in %TEMP% and is still adding them
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-12
closed:
---

## Problem

Found by the PR #869 reviewer while checking that PR's own cleanup, and independently re-counted by the
Foreman before filing. On this machine:

```
reviewer's count (19:0x)   144,699 cpe-* directories in %TEMP%
Foreman's count (19:2x)    145,207
```

It went up by ~500 in the minutes between the two counts, because the suite was running. **This is not a
historical mess that stopped growing; it is the current steady state.**

Worst offenders from the reviewer's breakdown:

| prefix | count |
|---|---|
| `cpe-binprev-pe-trunc` | 43,770 |
| `cpe-dotnetmeta-trunc` | 31,533 |
| `cpe-binprev-elf-trunc` | 29,298 |
| … | the balance |

Those names say what they are: per-case scratch directories from the truncation tests, one per case per
run, never removed.

## Why this is worth fixing rather than tolerating

1. **It makes "leave no orphans" unenforceable.** This sprint has repeatedly held individual tests to a
   zero-orphan standard — CPE-1678 leaked permanently-unreadable files and was fixed with a `Drop` guard;
   PR #869's new tests were checked for orphans across three separate red runs. That standard is being
   applied to new tests while ~145,000 directories from old ones sit next to them. A reviewer counting
   orphans has to filter out the noise to see the signal, which is exactly how the signal gets missed.
2. **It is a real resource problem.** Directory enumeration in `%TEMP%` degrades badly at this scale, and
   every tool that scans it — including this app's own explorer, antivirus, and backup software — pays.
3. **It hides the diagnostic value of what is left behind.** An orphan should mean something went wrong.
   When there are 145,000, it means nothing.

## Scope

`crates/server` primarily, but check every crate — the pattern is a `scratch()`-style helper that
`create_dir_all`s a uniquely-named directory and relies on a `remove_dir_all` at the end of the test, which
does not run when an assertion panics.

The fix that this repo has already converged on, twice, is a **`Drop` guard armed before the assertions**
(see `split_join.rs` and `dispatch.rs`). Applying it at the *helper* level rather than per test would fix
the whole class at once and stop the next test from reintroducing it: have `scratch()` return a guard type
that owns the directory and removes it on drop.

Consider also whether these tests need a temp directory at all — a truncation test that writes a byte
pattern and reads it back may be able to work in memory, which is faster and cannot leak.

**Clean up the existing 145,000** as part of this, and say how many were removed.

## Acceptance criteria

- [ ] A test that panics mid-assertion leaves **no** directory behind. Prove it by forcing a panic and
      counting before and after, with the real numbers in the PR.
- [ ] The fix is at the shared helper, not applied test-by-test — a newly written test that uses the
      helper cannot leak even if its author does not think about it.
- [ ] `%TEMP%` is cleaned of the existing `cpe-*` backlog, with the count reported.
- [ ] A full `cargo test` run across the workspace adds a net **zero** `cpe-*` directories. Count before,
      count after, put both numbers in the PR — this is the assertion that actually proves the class is
      closed, and it is cheap.
- [ ] Any test that *deliberately* leaves something behind (if one exists) says so explicitly.

## The count is still climbing, and PR #888 adds another producer

Measured by the PR #888 reviewer, 2026-08-13: the machine is now at **164,030** `cpe-*` directories in
`%TEMP%`, up from the **145,207** recorded on 2026-08-12. That is **~19,000 in a day**, which is this
sprint's own test runs.

It also identified a specific new producer to add to the site list: `crates/s3/src/provider.rs`'s
`spawn_s3_fixture_with_page_cap` does `std::env::temp_dir().join(..)` + `create_dir_all` with **no
cleanup** — measured at ~9 directories per `cargo test` run, 90 left behind by that review alone. It
copies `crates/webdav`'s pattern, so it is precedented rather than novel — which is rather the point of
this ticket.

The prescribed shape, from the same review: return an `impl Drop` guard from the spawner that removes the
root, rather than relying on the test to tidy up.

**Add `crates/s3` to whatever site list this ticket ends up carrying**, and check `crates/webdav` at the
same time since that is where the pattern was copied from.

## Notes

Filed by the Foreman from the PR #869 review, 2026-08-12, after independently reproducing the count and
watching it grow between two measurements.

Related: **CPE-1678** (the `Drop`-guard pattern this should generalise) and the Evidence Rules in
`Ticketing/wiki.md` — the guard-neutralisation rule mandates a red run per guard, which means every leaking
test leaks *by design of our own process*, once per ticket per developer.

## 2026-08-18: it has started failing tests, and the count is 1.29 million

Raised Medium → **High**. This is no longer a tidiness problem.

During the batched sprint of 2026-08-17/18 the count on this machine reached **~1,290,000** `cpe-*`
directories in `%TEMP%` — an order of magnitude past the 145,000 this ticket was filed at, which was itself
an order of magnitude past the figure in its own title.

**It caused a real test failure.** The CPE-1745 worker hit
`zip_lists_real_tree_and_extracts_inner_file` failing in `crates/server`, traced to a **PID collision**: so
many `%TEMP%/cpe-archive/<pid>-<seq>` directories are left behind that a reused process id now finds its
own scratch name already occupied. The test passed on an immediate rerun, which is the worst property a
failure can have — it teaches whoever sees it to hit rerun rather than read it.

That is the crossing point this ticket predicted. From its own Problem section: *"An orphan should mean
something went wrong. When there are 145,000, it means nothing."* At 1.29 million they no longer merely
mean nothing — they actively manufacture false failures, and they do it non-deterministically.

**Two further data points from the same sprint**, both arguing for the helper-level fix this ticket already
proposes rather than per-test cleanup:

- A review of PR #924 measured **five orphaned `cpe_test_cpe1715_*` trees** left by tests that cleaned up
  with a trailing `remove_dir_all` — and **one leaked even on a green run**, so the trailing call is not
  reliable when nothing panics either.
- Every PR merged this sprint had to be told individually to arm a `Drop` guard before its assertions.
  Three separate reviews raised it as a finding. That is the per-test standard being enforced by hand,
  ticket after ticket, while the helper that would make it automatic stays unwritten.

**Do the purge and the leak together.** A one-line purge clears the symptom and the flake; without the
`scratch()`-returns-a-guard change the count starts climbing again with the next test run, and the next
PID collision is only a matter of time.

### Second failure, hours later, and worse

`could not claim a private extraction directory ... after 1024 attempts`

That is `temp_extract_target`'s retry loop **exhausting its entire budget** — 1024 consecutive
`%TEMP%/cpe-archive/<pid>-<seq>` names all already taken. Not a collision it recovered from; a hard give-up.

It landed on CPE-1745's own brand-new test during a full parallel `cargo test` of `src-tauri`
(191 passed, 1 failed). The same test passed alone, and the whole suite passed serially with
`--test-threads=1` — so parallelism plus the backlog is what exhausts the namespace.

Two failures in one night, on two unrelated tickets, both environmental, both passing on rerun. The failure
mode is now **parallelism-dependent and non-deterministic**, which means it will surface most often on a
loaded CI runner and least often on the machine of whoever tries to reproduce it.
## Work Log — 2026-08-18, branch `CPE-1693-scratch-helper-owns-cleanup`

### The fix: `scratch()` returns a guard, at the helper level

Added `cpe_server::fsutil::ScratchDir` — a guard that owns a directory and removes it (with a bounded
retry, see below) on `Drop`, and `cpe_server::fsutil::scratch_dir(prefix)`, the shared constructor every
`scratch()` now delegates to. It is `pub`, not `#[cfg(test)]`-gated, for the same reason
`fsutil::make_dangling_link` isn't: `src-tauri`/`cpe-net`/`cpe-webdav`/`cpe-s3` need it from their own
test builds, and `#[cfg(test)]` is per-crate. `ScratchDir::adopt(path)` wraps an **already-created**
directory (for spawners with their own naming scheme, e.g. `cpe-s3`'s numbered fixture roots).

Every local `fn scratch(tag) -> PathBuf` / `fn scratch() -> PathBuf` helper across the tree (**67** of
them: 65 in `crates/server` — every `.rs` file that had one, plus its two `tests/*.rs` integration
files — one in `crates/net`, one in `src-tauri`) now delegates to `scratch_dir()` and returns
`ScratchDir` instead, with the **exact same directory-naming scheme** as before (so any tooling that
greps a `cpe-<module>-` prefix is unaffected). This was scripted (a Python transform matching the
near-identical boilerplate body every copy shared) and then compiler-driven: `cargo check --tests`
turned every call site that needed attention into a compile error, which is exactly why this is safe to
do mechanically — nothing that needed a human look could pass silently.

**Second-level wrapper helpers that create their own scratch dir and hand back a bare file path** needed
their own fix, since the *helper's* guard was dropping the directory the moment the helper returned,
before the caller could use it (E0308/borrow errors don't catch this — it compiled fine and failed at
runtime with a "path not found"):
- `crates/server/src/binary_preview.rs::write_temp_binary_info` and
  `crates/server/src/dotnet_metadata.rs::write_temp` now return `(ScratchDir, PathBuf)`; 6 and 11 call
  sites respectively updated to destructure and keep the guard bound.
- `crates/server/src/index.rs::sample_tree` (13 call sites) now returns `ScratchDir` directly (it was
  already handing back the whole directory, not a file inside it).
- `crates/server/src/batch_transform.rs::scratch_file` and `crates/server/src/thumb_pipeline.rs::scratch_file`
  had **no cleanup at all** before this ticket (not even a manual trailing `remove_dir_all`) — genuine,
  unconditional per-call leaks. Both now return `(ScratchDir, PathBuf)`.
- `crates/server/tests/finder_tags_os_interop.rs` and `crates/server/tests/native_meta_os_interop.rs`
  (macOS-only OS-interop tests) had a `scratch_file()` + manual `cleanup(path)` pair that never ran
  `cleanup` on a panic; both now use `scratch_dir` directly and the manual `cleanup` fn is gone.

**Now-redundant manual per-test `Drop` guards removed**, since `scratch()` itself is the guard now:
`archive.rs`'s `RemoveOnDrop` (CPE-1758), `split_join.rs`'s `RemoveOnDrop` (CPE-1729, 3 call sites),
and `src-tauri/src/lib.rs`'s `Cpe1715Scratch` (6 call sites) — each was `struct X(PathBuf); impl Drop
{ remove_dir_all }` wrapped manually around a `scratch()` call; removing the wrapper and keeping the now
self-cleaning `scratch()` result bound is the same behaviour with less code. `split_join.rs`'s ACL-deny
test keeps a slimmed `UndoDeny` guard (only the ACE-undo half — `ScratchDir`'s own `Drop` now does the
`remove_dir_all` half, and Rust drops locals in reverse declaration order so the ACE is undone first).

**`crates/s3/src/provider.rs`** (named explicitly in this ticket): `spawn_s3_fixture_with_page_cap`,
`spawn_s3_fixture`, `spawn_s3_fixture_without_listbucket`, and the `fixture_root()`/`s3_fixture_provider()`
helpers they compose with now return `cpe_server::fsutil::ScratchDir` (via `ScratchDir::adopt`, since
`fixture_root()` nests a numbered subdirectory under one shared per-test-binary-run parent rather than
using `scratch_dir`'s own naming). 28 call sites across `#[test]` fns updated (all already destructured
`let (base, root, requests) = spawn_...();` — safe to bind the guard in place of the old bare `PathBuf`).
The shared parent directory itself (`cpe-s3-fixtures-<pid>-<stamp>`) is deliberately left as one **empty**
per-test-binary-run entry — see Exemptions below.

**`crates/webdav/src/lib.rs`** (checked per this ticket's instruction, since `cpe-s3`'s fixture spawner
copied this file's pattern): `spawn_webdav_server_returning_root` and `spawn_webdav_server` now return
`(String, ScratchDir)` instead of `(String, PathBuf)` / `String`. Before this, `spawn_webdav_server`
threw its root away entirely without ever returning it — every one of its 9 call sites leaked
unconditionally, every call, with no exception. All 9 call sites (`let base = ...`) and all 9
`(base, root) = spawn_webdav_server_returning_root()` call sites updated.

### The retry: measured, not theoretical

The guard alone left a **real, measured residual leak** under a full-workspace `cargo test` — hundreds of
freshly created directories per run, concentrated in the binary-fixture truncation sweeps
(`binary_preview`/`dotnet_metadata`, which write synthetic PE/ELF/Mach-O files) plus a scattering of
ordinary single-scratch tests, even though every one of those same tests cleaned up perfectly when run in
isolation or in a small group. That isolation-vs-full-suite gap, concentrated on the fixtures that most
resemble real executables, is the signature of Windows Defender's real-time scanner transiently holding a
file handle open under heavy parallel `cargo test` load — documented on this machine already (see
`MEMORY.md`: "Defender quarantines test binaries... os error 225 is Defender, not a code fail"). A single
`remove_dir_all` attempt swallows that as a silent failure — the pre-CPE-1693 trailing
`let _ = fs::remove_dir_all(..)` calls had the identical exposure, so this is not a regression, just the
first time anything retries. `ScratchDir::drop` now retries `remove_dir_all` up to 5 times with a short
backoff (25ms × attempt) before giving up silently, same as every removal already did. **Before the retry:
a full `cargo test` in `crates/server` left ~531 fresh `cpe-*` directories behind (measured via `find`,
not the naive count below — see Methodology). After adding the retry: 0.**

### Methodology note — the naive `%TEMP%` count is not trustworthy at this scale

`%TEMP%` on this machine holds (at the time of this Work Log) **~294,800 pre-existing `cpe-*`
directories** — this ticket's own backlog, from before this fix. At that scale:
- `ls -d "$TEMP/cpe-"*` (shell glob expansion) **silently returns nothing** rather than erroring loudly —
  it looks like a clean "0" count when the glob simply failed. This produced a false "0 before / 0 after"
  reading early in this work that this Work Log does **not** rely on.
- The reliable count is `find "$TEMP" -maxdepth 1 -type d -name 'cpe-*' | wc -l` (no shell glob
  expansion), or, to isolate a **specific** run from a shared/concurrent machine (see below), a
  `-regextype posix-extended -regex '.*/cpe-[a-z0-9_-]+-[0-9]+-[0-9]+' -newermt '-N minutes'` scan
  immediately after the run, which only matches the `<prefix>-<pid>-<seq>` scratch-helper shape.
- This machine runs concurrent sprint activity (other CLI/desktop sessions; see `MEMORY.md`
  "Concurrent nightshift coordination"), so the **total** `%TEMP%` count drifts by small amounts between
  any two checks regardless of this ticket's fix. The trustworthy signal is not "did the grand total move
  by exactly zero" but "did **this specific test run** leave anything freshly created and not cleaned
  up," checked via the `-newermt` scan immediately after each run.

### Before/after counts (accurate method), one measurement per verified target

| Target | Feature mode | Tests | Fresh `cpe-<prefix>-<pid>-<seq>` dirs left by this run |
|---|---|---|---|
| `crates/server` | default | 2212 passed, 0 failed | **0** |
| `crates/server` | `--features index` | 2259 passed, 0 failed | **0** |
| `crates/server` | `--features pdf-thumb,video-thumb,waveform,dicom-thumb` | 2266 passed, 0 failed | **0** |
| `crates/net` | default | 37 passed, 0 failed | **0** non-exempt (5 exempt tags present — see below) |
| `crates/s3` | default | 195 passed, 0 failed | **0** numbered fixture roots (1 empty shared parent dir — see below) |
| `crates/webdav` | default | 32 passed, 0 failed | **0** |
| `src-tauri` | default | 188 passed, 0 failed | **0** |

Total `%TEMP%` `cpe-*` count immediately before vs. after a full `crates/server` default run:
**294,821 → 294,822** (net +1 across the whole machine, not this run specifically — the precise
`-newermt` scan for that exact run shows 0 fresh scratch-pattern directories; the +1 is background/
concurrent-machine noise, consistent with the Methodology note above).

### The panic-still-cleans-up proof

`crates/server/src/fsutil.rs::tests::scratch_dir_guard_removes_the_directory_even_when_the_caller_panics_mid_assertion`
— arms a `ScratchDir` guard, moves it into a closure, panics inside that closure via
`std::panic::catch_unwind`, then asserts (a) the panic actually happened and (b) the directory no longer
exists on disk. Real output from a run:

```
thread 'fsutil::tests::scratch_dir_guard_removes_the_directory_even_when_the_caller_panics_mid_assertion' (21648) panicked at src\fsutil.rs:2679:13:
CPE-1693 proof: deliberate panic — the guard above must already be armed
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test fsutil::tests::scratch_dir_guard_removes_the_directory_even_when_the_caller_panics_mid_assertion ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2215 filtered out; finished in 0.00s
```

The panic fires (proving the closure really panicked, not merely returned), and the test still reports
`ok` — meaning the post-unwind assertion that the directory is gone also passed.

### Sites exempted, and why

- **`crates/net/src/lib.rs`: `start_server`, `start_streaming_server`, and 3 inline test-local spawners**
  (`liststreambase`, `searchstreambase`, `contentstreambase`) each hand a scratch directory to a
  `HeadlessCtx` that is moved into a **detached** `std::thread::spawn` server loop with no join — the
  directory has to outlive both the spawning function and, in general, the calling test (the thread never
  stops on its own). These call `std::mem::forget` on the guard after extracting the path, which is
  **exactly the pre-existing documented behaviour** ("cleaned up by the OS temp reaper") — not a new
  leak, and not something this ticket's guard can safely auto-clean without risking pulling the rug out
  from under a still-running fake server. Verified via `-newermt` that these are the *only* `cpe-net-*`
  tags left behind by a full `cpe-net` test run.
- **`crates/s3/src/provider.rs`: the shared `cpe-s3-fixtures-<pid>-<stamp>` parent directory** —
  `fixture_root()` nests each fixture's numbered subdirectory under one parent shared across the whole
  test binary run (a prior round's mitigation, CPE-1684). Every numbered child is now guard-cleaned; the
  empty parent itself is left for the OS temp reaper (one empty directory per test-binary run, not one
  per test — down from every fixture's full content leaking every run). A `std::sync::OnceLock`-scoped
  parent has no natural single owner to hang a `Drop` guard on without adding process-exit machinery,
  which is out of proportion to what's left. Verified empty after a full run.
- **`crates/sftp/src/lib.rs` and `crates/ftp/src/lib.rs`** already carry their own manual
  `struct ScratchDirGuard(PathBuf); impl Drop { remove_dir_all }`, armed *before* assertions at each of
  their `#[test]` call sites (`let _guard = ScratchDirGuard(src.clone());`) — the same idiom
  `split_join.rs` used before this ticket. This already satisfies "a panic mid-assertion leaves no
  directory behind" for those specific tests; it is **not** the failure mode this ticket is about (a
  `scratch()`-style helper that leaks silently by default). Left as-is — converting them to the
  helper-level guard is a reasonable follow-up but is not named in this ticket's scope and risks scope
  creep into two crates the ticket never mentions.
- **`crates/server`'s "Tier 3" `std::env::temp_dir().join(..)` sites.**

  **Corrected 2026-08-19 (PR #934 re-review item 3).** This bullet used to say these sites "were never
  behind a `scratch()`-named helper to begin with". That is **factually wrong for four of them**, and the
  wrong reason is the actual hazard here — an inaccurate exemption is what stops the next person looking,
  which is precisely why CPE-1782 had to be filed later. `audit_journal::tmp`, `metrics_journal::tmp`,
  `replay_session::tmp` and `semantic_index::temp_path` are all **named helper functions**, and the first
  three are byte-for-byte the `scratch()` shape — `static SEQ: AtomicU64`, pid in the name,
  `create_dir_all`, return a bare `PathBuf` — differing only in being spelled `tmp` instead of `scratch`.
  That is exactly the name-based miss that finding 1 of this same review exists to correct.

  They are still exempted, and that is still a defensible call — but on the honest reason: they are a
  different, larger job than this ticket's mechanical conversion, each needing its own correctness read.
  What is *not* defensible is the previous wording implying nothing helper-shaped was left behind.
  `semantic_index::temp_path` in particular is **the largest single leaker measured on this machine at
  1,090 directories**, has no `remove_dir_all` anywhere in its file, and is one of the two survivors in
  this ticket's own post-fix measurement (`cpe-semidx-<pid>`). Anyone picking up the remaining leak should
  start there, not conclude from this list that the helper-shaped ones are all done.

  The genuinely ad-hoc inline sites — mainly `transfer.rs` (~25 individual literals, several
  already self-defending with a `remove_dir_all` *before* `create_dir_all` to survive a previous
  interrupted run), plus one-offs in `connections.rs`, `audit_journal.rs`, `metrics_journal.rs`,
  `model.rs`, `vector_index.rs`, `semantic_index.rs`, `vault_crypto.rs`, `fs_route.rs`, `provider.rs`,
  `replay_session.rs`. These are not the pattern this ticket's Scope section describes ("a
  `scratch()`-style helper"), and converting ~40 independent inline literals, each needing its own
  correctness read, is a materially different and much larger job than this ticket's mechanical
  conversion — attempting it here risked exactly the unreviewable, opportunistic-improvement diff the
  assignment warned against. **Recommend a follow-up ticket** if these are still worth closing once the
  helper-level fix (this ticket) has had a chance to show whether it alone drops the count enough.
- **No test in the converted set deliberately keeps its directory as its point** (i.e. asserts something
  about *leftovers*) — none were found; nothing was exempted for that reason.

### Purge command (recommended, **not run** by this worker)

Deleting ~294,800 directories is a real filesystem operation on the user's/Foreman's machine, scoped
here to exactly `%TEMP%\cpe-*` and nowhere else. A plain `Remove-Item -Recurse` loop is impractically slow
at this count; the script below stages every `%TEMP%\cpe-*` top-level entry into one temporary folder
(same-volume move, cheap) and then bulk-deletes that one folder in a single native recursive delete.

**Junction safety (PR #934 review finding 3), and why the mechanism changed.** The first draft of this
script bulk-deleted the staging folder with `robocopy /MIR` against an empty directory. The review flagged
that this follows a directory junction out of `%TEMP%` and deletes the junction's *target*, and proposed
adding `/XJ`. Measured on this machine: **`/XJ` does not fix it.** `/XJ` excludes junctions on robocopy's
*source* side only; `/MIR`'s destination purge still walks into a junction sitting in the destination tree
and deletes what's on the far side. Probe — a junction `staging\cpe-junction` pointing at `victim\`
containing `canary.txt`:

```
robocopy empty staging /MIR /XJ
        *EXTRA Dir        -1    ...\staging\cpe-junction\
          *EXTRA File            14    canary.txt
canary exists = False      <-- deleted through the junction, WITH /XJ
```

So the junction exclusion here is not a robocopy switch — it is a change of mechanism. The script now:

1. **Never stages a reparse point at all** — a top-level `%TEMP%\cpe-*` entry that is a junction/symlink is
   skipped, not moved, so nothing outside `%TEMP%` can ever be reached through the staged tree.
2. **Bulk-deletes with `rd /s /q`**, which deletes a junction *itself* rather than recursing through it.
   Verified with the same probe: `staging gone = True`, `canary survived = True`. It is one native
   recursive delete over one folder, so it keeps the speed the staging trick was for; it is `Remove-Item
   -Recurse` per directory that was slow, not `rd`.

Both points are commented in the script below — **do not "tidy" them away**; each one is load-bearing and
each is there because the alternative was measured deleting a file it had no business touching.

```powershell
# CPE-1693 purge — run this yourself; it is NOT executed by the ticket worker.
# Scope: ONLY top-level %TEMP%\cpe-* directories. Touches nothing outside %TEMP% and nothing whose name
# doesn't start with "cpe-".
$ErrorActionPreference = 'Stop'
$tempRoot = $env:TEMP
$staging  = Join-Path $tempRoot ("cpe-purge-staging-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $staging -Force | Out-Null

Get-ChildItem -Path $tempRoot -Directory -Filter 'cpe-*' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -ne $staging } |
    # JUNCTION EXCLUSION 1 of 2 — DO NOT REMOVE (CPE-1693 / PR #934 review finding 3).
    # A `cpe-*` entry that is a junction or symlink is left exactly where it is. Staging it would put a
    # door to somewhere else on the disk inside the folder we are about to delete recursively.
    Where-Object { -not ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) } |
    ForEach-Object {
        try { Move-Item -Path $_.FullName -Destination $staging -Force -ErrorAction Stop }
        catch { Write-Warning "Skipped $($_.FullName): $($_.Exception.Message)" }
    }

# JUNCTION EXCLUSION 2 of 2 — DO NOT REMOVE (CPE-1693 / PR #934 review finding 3).
# `rd /s /q` deletes a junction itself instead of following it; a nested junction inside a staged tree
# therefore costs its target nothing. This deliberately replaces `robocopy /MIR`, which followed one and
# deleted through it EVEN WITH `/XJ` (`/XJ` is source-side only — measured, see above). Do not swap this
# back to robocopy, and do not "simplify" it to `Remove-Item -Recurse` (slow at this count, and Windows
# PowerShell 5.1's recursive remove has its own history of walking into reparse points).
cmd /c rd /s /q "$staging"

Write-Host "CPE-1693 purge complete."
```

Count before/after with: `(Get-ChildItem -Path $env:TEMP -Directory -Filter 'cpe-*' -ErrorAction
SilentlyContinue | Measure-Object).Count`

### Local verification summary

`export PATH="$HOME/.cargo/bin:$PATH"` then, per crate:

- `crates/server`: `cargo clippy --all-targets -- -D warnings` / `--features index` / `--features
  pdf-thumb,video-thumb,waveform,dicom-thumb` — all clean. `cargo test` (all three feature modes) — all
  green (2212 / 2259 / 2266 passed, 0 failed).
- `crates/net`: `cargo clippy --all-targets -- -D warnings` clean; `cargo test` — 37 passed, 0 failed.
- `crates/s3`: `cargo clippy --all-targets -- -D warnings` clean; `cargo test` — 195 passed, 0 failed.
- `crates/webdav`: `cargo clippy --all-targets -- -D warnings` clean; `cargo test` — 32 passed, 0 failed.
- `src-tauri`: `cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`, `cargo clippy
  --all-targets --features sidecar-platform -- -D warnings` all clean; `cargo test` — 188 passed, 0
  failed. (Needed `npm ci && npm run build` first for `tauri_build`'s `dist/` requirement.)

Not run: `crates/sftp`/`crates/ftp` test suites (untouched by this ticket — see Exemptions), and the
Docker/E2E-gated legs of `crates/vfs`/`crates/s3`/`crates/webdav` (`e2e-extra-ca`, real-server
conformance) that need infrastructure this environment doesn't have.

## Work Log — 2026-08-19, PR #934 review response

Independent review returned CHANGES REQUESTED with four findings. Finding 4 (`crates/sftp`, `crates/ftp`,
`crates/net` still leaking) was split out as **CPE-1782** and is deliberately untouched here. The other
three are addressed below, each red-proofed: the test/probe is shown failing with the bug present and
passing with the fix.

### Finding 1 — the 68th helper: `src-tauri`'s `sched_scratch`

Census: every `fn … -> …Path…` in the five converted crates (`crates/server`, `crates/net`, `src-tauri`,
`crates/s3`, `crates/webdav`) whose body contains both `temp_dir()` and `create_dir…` — 8 functions. Seven
were left alone: `transfer::one_file_src` already returns its own guard; `ffmpeg_util::create_scratch_dir`
is production code that removes its own directory; four (`audit_journal::tmp`, `metrics_journal::tmp`,
`replay_session::tmp`, `semantic_index::temp_path`) are on this ticket's Tier-3 exemption list above; and
**`archive::temp_extract_target` leaks by design and is now tracked by CPE-1786** — see the correction
below, this Work Log first described it as "already guarded", which was wrong. Exactly one was a
`scratch()`-style helper the sweep should have caught and didn't: **`src-tauri/src/lib.rs`'s
`sched_scratch(tag) -> PathBuf`** (CPE-1199's snapshot-schedule tick tests, 3 call sites). It was missed
because the transform matched on the name
`scratch`, and this one is `sched_scratch`. Cross-checked: it is now the only `*scratch*`-named helper in
the tree that doesn't return `tempfile::TempDir` or `ScratchDir`.

Converted identically to the other 67 — `cpe_server::fsutil::scratch_dir(&format!("cpe-sched-tick-{tag}"))`,
which reproduces the previous `cpe-sched-tick-<tag>-<pid>-<seq>` naming exactly.

**Red-proof** — `cd src-tauri && cargo test --lib sched_scratch_removes`, with the helper still returning
a bare `PathBuf` (the new test compiles either way, so this is a genuine before/after, not a compile
error):

```
test tests::sched_scratch_removes_its_directory_even_when_the_caller_panics_mid_assertion ... FAILED
CPE-1693 REGRESSION: C:\...\Temp\cpe-sched-tick-guard-proof-2736-0 still exists after sched_scratch's
value went out of scope on a panicking unwind — this helper still hands back a bare path with no Drop guard
test result: FAILED. 0 passed; 1 failed
```

After the conversion: `test result: ok. 1 passed; 0 failed`.

The rebase onto `main` also turned up CPE-1708's `list_dir_local_arm_always_reports_zero_filtered`, which
had hand-rolled a `Cleanup(PathBuf)` guard around `scratch()`'s old bare return and called `dir.clone()`.
`ScratchDir` is deliberately not `Clone`, so it stopped compiling; the wrapper is redundant now and was
removed — which is exactly the boilerplate this ticket exists to delete.

### Finding 2 — `ScratchDir::adopt` was an unconstrained recursive-delete primitive

`adopt(path)` took *any* `PathBuf` and armed `remove_dir_all` on it at drop. It now canonicalises both the
argument and the scratch root (`std::env::temp_dir()`) and refuses anything that is not an existing
directory *strictly* inside that root — panicking with the offending path, since every caller is a test
fixture and there is nothing to recover to. Canonicalising first is what makes it junction/symlink-proof:
a link inside `%TEMP%` whose target is elsewhere resolves out of the root and is refused. The root itself
is refused separately, because `starts_with` accepts a path as its own prefix and a guard over `%TEMP%`
would take every other test's scratch space with it on drop.

Both existing callers (`cpe-s3`'s `fixture_root`, `cpe-webdav`'s `spawn_webdav_server`) build their paths
under `std::env::temp_dir()` and are unaffected.

**Red-proof** — `cd crates/server && cargo test --lib adopt_`, before the constraint. The out-of-root probe
plants a canary file and checks it survived, so it stays red both if `adopt` doesn't refuse and if it
"refuses" only after its guard has already eaten the directory:

```
test fsutil::tests::adopt_refuses_a_path_that_does_not_exist ... FAILED
test fsutil::tests::adopt_refuses_the_scratch_root_itself ... FAILED
test fsutil::tests::adopt_refuses_a_path_outside_the_scratch_root ... FAILED
test fsutil::tests::adopt_accepts_a_directory_nested_inside_the_scratch_root ... ok
  ScratchDir::adopt accepted …\target\debug\deps\cpe-1693-adopt-probe-outside-4584-0, which is outside
  the scratch root — adopt() is an unconstrained recursive-delete primitive again
test result: FAILED. 1 passed; 3 failed
```

After: `test result: ok. 4 passed; 0 failed`. The fourth (positive) test is there so the constraint can't
regress into "refuse everything".

### Finding 3 — the purge script followed junctions out of `%TEMP%`

The review was right about the hazard and wrong about the remedy, which the probe caught: **`/XJ` does not
fix `robocopy /MIR`**. `/XJ` is a *source*-side exclusion; the `/MIR` destination purge still walks into a
junction in the destination tree and deletes through it. Measured with a junction
`staging\cpe-junction` → `victim\canary.txt`:

```
robocopy empty staging /MIR /XJ
        *EXTRA Dir        -1    ...\staging\cpe-junction\
          *EXTRA File            14    canary.txt
canary exists = False        <-- deleted through the junction, WITH /XJ
```

So the junction exclusion had to be a change of mechanism, not a switch. The script now (a) skips any
top-level `%TEMP%\cpe-*` entry that is a reparse point instead of staging it, and (b) bulk-deletes staging
with `rd /s /q`, which removes a junction itself rather than recursing through it. Both are commented
in-script as load-bearing, with the measurement, so they don't read as noise later.

**Red-proof** — end-to-end probe over a `%TEMP%` stand-in containing a real `cpe-real\sub\junk.txt` tree, a
top-level `cpe-toplevel-junction`, and a nested `cpe-real\sub\cpe-nested-junction`, both pointing at an
out-of-temp `outside\precious\canary.txt`. Old script (with the reviewer's `/XJ`) vs new script, same
fixture:

```
variant                       = old
canary OUTSIDE temp survived  = False      <-- data loss outside %TEMP%
cpe-real\sub\junk.txt purged  = True
cpe-real purged               = True

variant                       = new
canary OUTSIDE temp survived  = True       <-- fixed
cpe-real\sub\junk.txt purged  = True       <-- and the purge still purges
cpe-real purged               = True
```

## Work Log — 2026-08-19, PR #934 second re-review

Four items. Three were correct as raised and are fixed; the fourth is a correction to this ticket's own
claims, which were wrong.

### Re-review item 1 — `adopt()` was defeated: what was checked was not what was deleted

`adopt` validated `canonicalize(path)` and then constructed `ScratchDir(path)` from the **raw argument**.
A 9-attack probe got 8 refusals (`..` traversal, an NTFS junction inside `%TEMP%`, the string-prefix
sibling `Temp-cpe1693evil` vs `Temp`, a non-existent path, a `\\?\` path, forward slashes with a trailing
separator) and **one defeat**: `adopt("victim")` with the current directory inside `%TEMP%` was accepted,
and the out-of-root canary died.

Fixed twice over: a relative argument is now refused up front *by name*, and the guard stores the
**resolved** path so `Drop` removes exactly what containment was checked against. No current caller was
ever exposed — `cpe-s3`'s `fixture_root` and `cpe-webdav`'s spawner both pass absolute `temp_dir()`-rooted
paths, and `set_current_dir` has zero hits in the tree — so this was a latent gap in the primitive, not a
live bug. It still had to close: finding 2's whole premise is a test-support API where one wrong argument
wipes something real, and a wrong argument still could.

Storing `canonicalize`'s output verbatim then broke two of `cpe-webdav`'s CPE-1726/CPE-1730
root-replacement tests, because on Windows that is an extended-length `\\?\C:\…` path which normalises to
`//?/C:/…` and no longer reads as absolute to their fixtures. `de_verbatim` strips the prefix for the
plain drive form only (leaving `\\?\UNC\server\share` alone, where stripping would yield a *different*
path), so the stored path is still fully resolved — no `..`, no unresolved links — but spelled the way
`scratch_dir` spells the other 70 helpers' paths. `cpe-webdav` back to 32 passed, `cpe-s3` 195 passed.

Both tests are deliberately **cwd-free**: `set_current_dir` is process-global and would race every other
test in this parallel harness. One adopts a non-canonical absolute spelling (`…/child/../child`) of a
legitimate in-root directory and asserts the stored path has no `ParentDir` component left; the other
asserts the relative refusal fires *by name*, matching on the panic message, because pre-fix a relative
probe panicked anyway from `canonicalize` failing — an accident of the probe path not existing rather than
a refusal of relative paths.

### Re-review item 2 — there was a 69th and a 70th, and they were worse

`src-tauri`'s `make_watch` (CPE-1099) and `make_index_watch` (CPE-1138). The first census filtered on
**return type**, and these return a domain struct (`AgentWatch` / `IndexWatch`), not a path — so it was
structurally incapable of seeing them. That is the real lesson: a census keyed on the shape of the
*return value* misses every helper that hands the directory to something else.

They are worse than the 68th. `sched_scratch` had a trailing `remove_dir_all` at all three call sites, so
it leaked only on a panicking assertion; **nothing anywhere removed these**, so they leaked on every green
run. Measured on a fresh synthetic temp root, the two *passing* tests left **6 directories** behind. On
this machine's real `%TEMP%`: `cpe1099-*` = 138, `cpe1138-*` = 99 (counted directly, both figures
confirmed). Neither prefix matches the purge script's `cpe-*` filter, so those 237 were **unpurgeable as
well as uncounted** — the new `cpe-agent-watch-` / `cpe-index-watch-` prefixes bring them back in scope.

Both now return `(ScratchDir, Watch)`. At each call site the guard vector is declared **before** the state
map so it drops **after** it — locals drop in reverse declaration order — so no directory is removed out
from under a live `notify` watcher still holding a handle on it. Same probe after the change: **6 → 0**,
both tests still green, synthetic temp root completely empty.

### Re-review item 4 — `archive::temp_extract_target` is NOT "already guarded". Correcting the record.

This Work Log dismissed `archive::temp_extract_target` as "already guarded". **That was wrong**, and it
conflated two unrelated properties. CPE-1733 gave it an exclusive `create_dir` to defend against
**squatting/redirect**. That says nothing about **leak-freedom**, and the function's own doc comment says
the opposite in as many words:

> **Nothing here cleans up.** CPE-1693 tracks the 145,000 leftover directories; this change adds one more
> directory per extraction just as before.

CPE-1745's Done record said the same: *"CPE-1693 tracks the leak; not touched by this ticket."* Two
reviews and this Work Log all read past it.

**The counting-method trap, which is what concealed it.** Counting top-level `%TEMP%\cpe-*` shows `+1`
after a full run, which looks like a rounding error. That `+1` is the single `cpe-archive` **parent**;
every extraction's directory is one level *down* inside it. Counted one level down, an independent UAT
measured **1,394,403 directories**, growing `+12` from a single 8-way-parallel module run. Independently
re-checked here: `cpe-archive` exists and its child enumeration passes **200,000 without finishing** (walk
capped deliberately). That is essentially the entire 1.29M crisis figure, alive, in production code — and
it is the site of *both* failures that escalated this ticket to High (the PID collision, and
`temp_extract_target`'s 1024-attempt exhaustion).

**Demonstrated from a clean slate**, which removes any doubt that this is historical accumulation. A full
green `crates/server` run against a **fresh, empty** synthetic temp root (`TMP`/`TEMP` redirected, exit 0):

```
leftover top-level dirs = 2
  cpe-archive          (children: 2681)
  cpe-semidx-23800     (children: 0)
```

Both counting methods on the same single run: **2** by the metric this ticket has been reporting, **2,683**
by the one that actually reflects what is on disk. The post-fix "2 survivors" figure is true and is also
exactly the trap — the two survivors are precisely the two leaks left open, `cpe-archive` (CPE-1786) and
`semantic_index::temp_path` (Tier 3). One run of one crate creates 2,681 directories nothing will ever
remove, and no `cpe-*`-filtered purge reaches them because they are nested one level down.

Filed as **CPE-1786** (High). Any future measurement of this class must count **inside** `cpe-archive`,
not just top-level `cpe-*`, or it will report success it has not earned.

### What this ticket actually closes — narrowed

Previously claimed: "closes the temp-dir leak". Corrected to what the evidence supports:

> **Closes the temp-dir leak at the test-helper level.** 70 `scratch()`-style helpers across five crates
> now return a `Drop` guard, taking a full `crates/server` test run from **532 leaked directories to 2**
> (99.6%), with a panic-mid-assertion proof test and a junction-safe purge script for the existing
> backlog.

Explicitly **not** closed by this ticket, and tracked elsewhere:

| Left open | Where | Scale |
|---|---|---|
| `archive::temp_extract_target` — production extraction path, leaks by design | **CPE-1786** | ~1.39M inside `cpe-archive` |
| `crates/sftp`, `crates/ftp`, `crates/net` test scratch | **CPE-1782** | first re-review's finding 4 |
| Tier-3 helpers, `semantic_index::temp_path` worst | exemption list above | 1,090 (`cpe-semidx-*`, confirmed) |
