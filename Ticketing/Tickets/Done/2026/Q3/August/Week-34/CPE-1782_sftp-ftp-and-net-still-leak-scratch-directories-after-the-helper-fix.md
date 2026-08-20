---
id: CPE-1782
title: sftp, ftp and net still leak scratch directories after the helper-level fix
type: task
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-18
closed:
---

## Problem

CPE-1693 made `scratch()` return a `Drop` guard and converted the helpers across `crates/server`,
`crates/net`, `src-tauri`, `crates/s3` and `crates/webdav` — measured by the reviewer at **532 leaked
directories per full `crates/server` test run down to 2**. Three crates were left out, and the review found
the stated reason for two of them was inaccurate.

### 1 & 2. `crates/sftp` and `crates/ftp` — the exemption was overstated

CPE-1693's Work Log claimed both crates "already carry their own manual `ScratchDirGuard` … armed *before*
assertions **at each of their `#[test]` call sites**". Measured state:

- `ScratchDirGuard` exists (`sftp:1233`, `ftp:900`) but is used at **3 sites each** — `sftp:1250,1275,1301`
  and `ftp:916,939,971`.
- `crates/sftp/src/lib.rs:1028` `scratch_known_hosts_path(tag) -> PathBuf` is a textbook `scratch()`-style
  helper returning a bare path, whose callers do
  `let _ = std::fs::remove_dir_all(store.parent().unwrap());` **after** their assertions — panic-unsafe,
  exactly the failure mode CPE-1693 exists to close.
- 13 raw `std::env::temp_dir().join(..)` sites across the two files (9 sftp, 4 ftp), of which **6** are
  guarded.
- Neither crate uses `tempfile`/`TempDir` anywhere, despite `tempfile = "3"` being a dependency of
  `crates/sftp`.

Being out of scope was a fine answer. Recording it as "already satisfied" is what stops the next person
looking, which is why this ticket exists.

### 3. `crates/net` — five deliberate `mem::forget`s

`start_server` and `start_streaming_server` (`crates/net/src/lib.rs:74,365,419,458,498`) `mem::forget` their
scratch guard. The rationale is genuine, not a fig leaf: they return only a `SocketAddr`, the context is
moved into a detached `thread::spawn` with no join, so a guard armed inside those functions would delete the
server's data directory while the server is still running. Arming it there would be a real bug, and each
site carries an explanatory comment.

But it is **not forced**. `start_server` has 13 `#[test]` callers and `start_streaming_server` has 6.
Returning `(SocketAddr, ScratchDir)` and letting the *caller* own the guard gives the directory exactly the
lifetime it needs. The detached thread would then serve against a deleted root — which is fine, because
nothing connects to it once the test returns.

**That is precisely the shape CPE-1693 chose for `crates/webdav` and `crates/s3`**, which have the same
detached-thread structure. Applying it in two crates and not the third is an inconsistency, and
`mem::forget` inside a ticket whose entire purpose is "stop deliberately leaking" reads worse than it is.
~19 directories per `cpe-net` run.

## What to do

- Convert `scratch_known_hosts_path` and the unguarded `temp_dir().join(..)` sites in `crates/sftp` and
  `crates/ftp` to `cpe_server::fsutil::scratch_dir`, matching what the other five crates now do. Delete the
  trailing manual `remove_dir_all` calls — the guard covers the panic path, which they never did.
- Return `(SocketAddr, ScratchDir)` from `crates/net`'s two spawners and drop all five `mem::forget`.
- **Bind every guard to a named local.** `let _guard = …` keeps it alive; `let _ = …` drops it immediately
  and silently disarms everything. The CPE-1693 review checked all ~90 sites specifically for this.
- Measure before and after, per crate, with a method that cannot under-count — `find` with `-newermt`
  against a marker file, **not** a shell glob. A glob silently under-reports once `%TEMP%` has hundreds of
  thousands of entries, and produced a false "0 leaked" reading during CPE-1693.

## Acceptance criteria

- [ ] A full `cargo test` of `crates/sftp`, `crates/ftp` and `crates/net` each leaves **zero** new
      directories in `%TEMP%`. State the measurement method and the real before/after numbers per crate.
- [ ] No `mem::forget` of a scratch guard remains in `crates/net`.
- [ ] No guard is bound to a bare `_`.
- [ ] A test that panics after arming its guard still cleans up — demonstrate in at least one converted
      crate.
- [ ] Any site genuinely unable to own its guard is documented at the call site with the reason, not
      recorded as already-solved.

## Notes

Found by the Reviewer on **PR #934 / CPE-1693**, 2026-08-18, during the batched sprint. Related: CPE-1693
(the helper-level fix and its measurement), CPE-1731 (the honest-rig work in these same two crates),
CPE-1742.

## Work Log — 2026-08-19 (Foreman, at merge)

Merged as PR #941 with an `APPROVE` from an independent Reviewer and a `UAT PASS`, both of which
measured the result themselves rather than trusting the PR.

**Measured leak counts, per crate, one full `cargo test` run against a fresh empty temp root** (the PR
body's own table was mislabelled — both its columns were post-fix, so it contained no baseline):

| Crate | Before | After |
|-------|--------|-------|
| `crates/sftp` | 46 | **0** |
| `crates/ftp` (default) | 33 | **0** |
| `crates/ftp` (`e2e-extra-ca`) | — | **0** |
| `crates/net` | 20 | **0** |

**99 leaked directories per green run → 0.** The leaked set on `main` was exactly the server roots,
confirming this ticket's diagnosis that the old trailing `remove_dir_all` calls did clean the other
directories on a green run and never cleaned the spawner roots at all.

**Independent census found no straggler.** Swept by neither name nor return type, per the lesson
CPE-1693 paid for. The only remaining `temp_dir()` sites in these three crates are
`crates/net/examples/security_demo.rs:22` and `crates/net/src/bin/cpe-server-ref.rs:27` — both non-test
server/demo data directories that should not self-delete, and never executed by `cargo test`.

**The `crates/net` lifetime question was answered by experiment, not argument.** The UAT started a real
server, dropped the guard early while the thread was live and a client still connected, and showed the
deleted-root call fails cleanly with the connection still usable — then ran 30 clean passes at 16
threads. The Reviewer separately confirmed all 17 call sites bind the guard *before* the client, so
reverse-declaration drop closes the connection first and removes the root second.

**Panic safety proven in all three crates.** The PR shipped one proof (sftp); the UAT wrote its own for
ftp and net and confirmed `Drop` runs during unwind in each. A force-kill still leaks — no code runs
after `TerminateProcess` — but the leftover names remain `cpe-*`-prefixed and so stay purgeable, which
is the documented limitation rather than a defect.

**Known leftovers, deliberately not done here** (all raised as non-blocking by the Reviewer):
`crates/net/src/lib.rs:57`'s doc comment still says "cleaned up by the OS temp reaper", which stopped
being true when CPE-1693 landed; AC #5 wants a one-line reason at the two non-test `temp_dir()` sites;
and `crates/webdav/src/lib.rs:1975` `cpe_1730_seed_victim_outside` is still the un-guarded shape — it
leaks only on the panic path, which is why nobody has counted it. Filed as **CPE-1793**.
