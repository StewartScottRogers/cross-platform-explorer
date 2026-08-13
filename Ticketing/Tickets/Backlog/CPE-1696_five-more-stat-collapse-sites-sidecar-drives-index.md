---
id: CPE-1696
title: Five more stat-collapse sites — sidecar resolution, drive listing, and search-index tombstoning
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

The **fifth** round of the same bug: an unknown answered as a confident one. Found by the CPE-1692
worker's fresh sweep (292 hits across `crates/`, `src-tauri/` and `sidecar/`), and deliberately left
unfixed there rather than scope-creeping a PR that was already fixing eight sites — the right call.

Five live sites, all collapsing a `stat` outcome into an existence claim:

- `src-tauri/src/lib.rs` — **sidecar-binary resolution**. A denied or unreadable sidecar path is
  reported as a missing sidecar. Given [[install-kill-all-processes-first]] and how often a locked or
  half-installed sidecar is the actual cause, "it isn't there" is precisely the wrong diagnosis.
- `src-tauri/src/lib.rs` — **drive listing**. A drive that cannot be stat'd (a disconnected network
  drive, a card reader with no card, a BitLocker volume that is locked) is treated as absent.
- `crates/server/src/index_watch.rs` — **search-index tombstoning**. This is the one with teeth: if a
  transient stat failure reads as "the file is gone", the watcher **tombstones a file that still
  exists** and it drops out of search results until something re-indexes it. A permission blip becomes
  silent data loss from the user's point of view.

## Why this keeps happening

Read CPE-1692's "Why this was missed twice before" section — the structural cause has now recurred four
times: **the broader search had the narrower scope.** Each sweep was honest about what it searched and
still concluded something wider than its search supported.

These five were found only because the CPE-1692 worker ran the sweep across `sidecar/` as well as
`crates/` and `src-tauri/`, which no previous pass had done.

`Path::try_exists()` — which returns `io::Result<bool>` instead of collapsing every failure into
`false` — had **zero** uses across the repo before CPE-1692. Prefer it, or a `metadata()` call
classified through `dispatch::classify_path_error`, at every site here.

## Scope

The five sites listed above. **Do not** re-open the ~20-site `is_dir()` type-check family — CPE-1692
made an explicit, documented decision to leave it (a type claim is a smaller lie than an absence
claim), recorded in a `dispatch.rs` comment.

## Acceptance criteria

- [ ] Each of the five sites distinguishes a genuine absence from a stat failure, reusing
      `classify_path_error`'s taxonomy rather than re-deriving one.
- [ ] The index-watch site does **not** tombstone on a non-NotFound stat failure. A test proves a
      transient failure leaves the index entry intact — this is the acceptance criterion that matters
      most, because its failure mode is invisible to the user until a search comes back short.
- [ ] A genuinely missing path still reports not-found from every site.
- [ ] Tests drive the real entry points, not the helpers.
- [ ] **Read CPE-1692's findings on constructing the denied condition before writing a test.** It
      measured that a parent-directory traversal deny does **not** work on Windows — the default
      "bypass traverse checking" privilege defeats it — so its end-to-end tests skip on Windows and run
      for real on Unix in CI's 3-OS matrix. Do not rediscover that the hard way.
- [ ] Any skip **announces itself** with `writeln!(std::io::stderr(), ..)` — not `eprintln!`, which
      libtest swallows for passing tests — confirmed visible under plain `cargo test`.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR.
- [ ] Run the sweep again, wider than this ticket's scope, and **state the exact scope you searched**.
      If it comes back clean, that is only ever "clean within X" — name X.

## Notes

Filed by the Foreman from the PR #874 work, 2026-08-12.

Related: **CPE-1678**, **CPE-1687**, **CPE-1692** (the same bug, three times before this), **CPE-1673**
(the taxonomy), and the Evidence Rules in `Ticketing/wiki.md`.
