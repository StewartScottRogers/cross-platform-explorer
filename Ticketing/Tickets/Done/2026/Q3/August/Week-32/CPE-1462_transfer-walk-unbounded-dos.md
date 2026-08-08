---
id: CPE-1462
title: "Unbounded recursion + accumulation in transfer::walk/download_tree → memory-exhaustion / hang DoS from a hostile remote server"
type: Bug
status: Done
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

## Work Log (2026-08-08, Done — PR with CPE-1461 on branch `cpe-1461-1462-remote-traversal-dos`)
Bounded `walk` and un-collected `download_tree`, both in `crates/server/src/transfer.rs`.

**Caps in `walk`.** The DFS stack now carries per-item depth. Two `pub const` caps:
- `MAX_WALK_DEPTH = 100` — reaching it stops descent into *deeper* dirs (surfaced `eprintln!` notice), the
  rest of the walk continues. 100 is far above any real remote tree (real paths rarely exceed ~40 levels)
  while firmly bounding recursion, so the "one fresh child dir per readdir → infinite depth" attack
  terminates. Justified as skip-descent (not hard-fail) to match the repo's list_dir skip-on-error ethos.
- `MAX_WALK_ENTRIES = 500_000` — exceeding the total visited count aborts the whole walk with a surfaced
  `Err` ("…safety cap…"). Hundreds of thousands covers a legitimate large tree; a bounded FAILED transfer
  is vastly better than an OOM/hang on an unattended download. This catches the breadth attack (one dir
  with millions of children) and any depth×breadth combination the depth cap alone wouldn't.

**Streaming `download_tree`.** No longer collects the entire tree into a `Vec<WalkEntry>` before writing.
It writes each entry straight to disk inside the `walk` callback as it is discovered (the callback borrows
the provider immutably for `read`, alongside `walk`'s own immutable borrow), so accumulation is bounded
regardless of tree size. Combined with the CPE-1461 `guarded_join`, each streamed write is also
containment-checked.

**Symlink-loop guard.** Checked: `ProviderEntry` carries no symlink signal (only `name`/`is_dir`/`size`),
and `walk` operates on provider-reported paths, not OS real paths — a hostile SFTP/WebDAV server can simply
lie in readdir. So there is no real-path to track and no `Component`-level symlink to detect; the depth +
entry caps are what bound a server-advertised loop (a dir that always re-lists itself/one child). Noted
here rather than adding an inapplicable realpath tracker. The upstream russh-sftp per-listing accumulation
noted in the ticket is left as an upstream/per-listing-cap follow-up (out of scope for the shared walker).

**Tests (green):** `walk_depth_cap_terminates_an_infinitely_deep_tree` (an `InfiniteDepth` provider that
returns one fresh child dir forever → walk returns bounded ≤ depth+1) and `walk_entry_cap_aborts_a_huge_tree`
(a `HugeTree` provider = 1000 dirs × 1000 files → `Err` at the entry cap). Existing download/upload
round-trips still pass (streaming path is correct, no regression). Build + `clippy --all-targets -D warnings`
clean; `walk`/`download_tree` public signatures unchanged.
