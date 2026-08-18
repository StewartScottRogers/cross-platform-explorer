---
id: CPE-1782
title: sftp, ftp and net still leak scratch directories after the helper-level fix
type: task
priority: Medium
status: Backlog
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
