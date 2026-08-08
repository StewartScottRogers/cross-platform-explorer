---
id: CPE-1462
title: "Unbounded recursion + accumulation in transfer::walk/download_tree → memory-exhaustion / hang DoS from a hostile remote server"
type: Bug
status: Backlog
priority: Medium
component: Backend
tags: [ready, security]
epic: CPE-616
created: 2026-08-08
---
## Vector (found in the crates/sftp/vfs deep audit, 2026-08-08)
`crates/server/src/transfer.rs:~37-64` `walk` has NO depth cap and NO visited-count cap; `:~75-76`
`download_tree` does `walk(..., |e| entries.push(e))` — collecting the ENTIRE tree into a `Vec` before writing
anything.

## Concrete malicious input
A hostile server that, for READDIR of any directory, always returns one entry marked `is_dir` with a fresh name
(`a`, then `a/a`, then `a/a/a`, …). `walk`'s DFS stack + `visited` + `download_tree`'s `entries` Vec all grow
without bound → OOM / indefinite hang. No symlink required (the server just lies in readdir); a symlink-to-parent
advertised as a directory does the same. Compounding: russh-sftp's own `read_dir` accumulates ALL entries of one
directory into a Vec (session.rs:183-204), so a server advertising millions of entries in one dir OOMs even inside
`provider.list()` — worth an upstream note / a client-side per-listing cap.

## Existing mitigation
Only the cooperative `cancel` flag (checked per dir/entry) — useless against an automatic transfer the user isn't
watching.

## Fix direction
Add a max-depth cap AND a max-entry-count cap to `walk` (error, or truncate with a surfaced notice, when
exceeded). In `download_tree`, STREAM entries to disk as they're walked instead of collecting the whole tree into
`entries` first. Add a symlink-loop guard (track visited real paths) if providers can report symlinks. Consider a
per-listing entry cap for `provider.list()` against the upstream readdir accumulation.

## Effort / blast radius
S / caps + streaming in transfer.rs. Serialize with CPE-1461 (same file). Epic CPE-616.
