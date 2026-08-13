---
id: CPE-1696
title: Nine more stat-collapse sites — two of them fail open into a silent overwrite
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-12
closed: 2026-08-13
---

## Problem

The **fifth** round of the same bug: an unknown answered as a confident one. Found by the CPE-1692
worker's fresh sweep (292 hits across `crates/`, `src-tauri/` and `sidecar/`), and deliberately left
unfixed there rather than scope-creeping a PR that was already fixing eight sites — the right call.

**Nine** live sites, all collapsing a `stat` outcome into an existence claim. Five were found by the
CPE-1692 worker's sweep; four more by the PR #874 reviewer's wider one.

The first five:

- `src-tauri/src/lib.rs` — **sidecar-binary resolution**. A denied or unreadable sidecar path is
  reported as a missing sidecar. Given [[install-kill-all-processes-first]] and how often a locked or
  half-installed sidecar is the actual cause, "it isn't there" is precisely the wrong diagnosis.
- `src-tauri/src/lib.rs` — **drive listing**. A drive that cannot be stat'd (a disconnected network
  drive, a card reader with no card, a BitLocker volume that is locked) is treated as absent.
- `crates/server/src/index_watch.rs` — **search-index tombstoning**. This is the one with teeth: if a
  transient stat failure reads as "the file is gone", the watcher **tombstones a file that still
  exists** and it drops out of search results until something re-indexes it. A permission blip becomes
  silent data loss from the user's point of view.

### Four more, added from the PR #874 review

The PR #874 reviewer swept **all 570 tracked `.rs` files** — wider than any pass before it — and found
four the CPE-1692 worker's sweep missed. Two of them **fail open into a silent overwrite**, which puts
them above the original five in severity:

- `crates/server/src/batch_execute.rs:199` — `is_foreign_overwrite`. `if !Path::new(&item.output).is_file()
  { return false }` — "nothing sits there yet". A denied stat therefore reads as *nothing to overwrite*
  and `execute_plan_walk` **writes anyway**. The function's own header comment is titled *"Security
  audit finding 4 (PR #848). Standalone, this function fails OPEN…"* — PR #848 closed the ADS route and
  left the stat-collapse route open.
- `src-tauri/src/lib.rs:452-482` — `unique_target`, three collapses (`:454`, `:474`, `:479`). Its doc
  comment reads *"We never overwrite an existing file — silent overwrite is data loss."* A denied stat
  on the candidate returns it as free, and the copy overwrites.
- `crates/server/src/thumb_video.rs:173` — `if !out.exists() { Err("ffmpeg reported success but produced
  no output file") }`. Low impact, same shape.
- `crates/server/src/transfer.rs:109` — `existing_ancestor`, `p.symlink_metadata().is_ok()`. This is the
  **CPE-1461 symlink-escape guard**. A denied `lstat` makes it walk past the deepest existing component
  and containment-check a shallower ancestor, so a symlink at the skipped level goes unverified before
  `create_dir_all` follows it. The reviewer flagged this at lower confidence — mitigated in practice,
  since a path you cannot `lstat` you probably cannot traverse — but it is a security guard, so treat it
  as the one to reason about hardest.

## Read this before writing a Windows test — `exists()` and `try_exists()` are different syscalls

The single most useful thing to come out of PR #874, established by measurement on non-elevated local
NTFS:

- `Path::exists()` is `metadata().is_ok()` → `CreateFileW` with desired-access **0**, which **no deny ACE
  refuses**. A parent-directory deny cannot make it fail on Windows (bypass-traverse-checking).
- `Path::try_exists()` is `fs::exists()`, an **attributes query**, which a deny ACE **does** refuse.
  `icacls <target> /deny <user>:(F)` on the target itself produces a real `PermissionDenied`.

So a Windows permission test **is** constructible — but only if the probe that decides whether the deny
took effect uses **the same call as the code under test**. CPE-1692's first attempt probed with
`fs::metadata` while the code called `try_exists`, so every leg skipped and the test had zero power over
the bug it existed to catch. A mismatched probe is worse than no test: it turns an uncovered case into a
covered-looking one.

## Why this keeps happening

Read CPE-1692's "Why this was missed twice before" section — the structural cause has now recurred four
times: **the broader search had the narrower scope.** Each sweep was honest about what it searched and
still concluded something wider than its search supported.

The first five were found only because the CPE-1692 worker swept `sidecar/` as well as `crates/` and
`src-tauri/`, which no previous pass had done. The next four were found only because the PR #874
reviewer went wider again — all 570 tracked `.rs` files, including `tests/`, `examples/`, `benches/`,
`src/bin/` and `src-tauri/build.rs`, and searching inverted forms (`if x.is_dir() { .. } else { Err(..) }`)
that a leading-`!` pattern misses. **Each widening of the scope has found more.** Assume this list is
still incomplete.

`Path::try_exists()` — which returns `io::Result<bool>` instead of collapsing every failure into
`false` — had **zero** uses across the repo before CPE-1692. Prefer it, or a `metadata()` call
classified through `dispatch::classify_path_error`, at every site here.

## Scope

All nine sites listed above. **Do not** re-open the ~20-site `is_dir()` type-check family — CPE-1692
made an explicit, documented decision to leave it (a type claim is a smaller lie than an absence
claim), recorded in a `dispatch.rs` comment.

## Acceptance criteria

- [ ] Each of the nine sites distinguishes a genuine absence from a stat failure, reusing
      `classify_path_error`'s taxonomy rather than re-deriving one.
- [ ] The index-watch site does **not** tombstone on a non-NotFound stat failure. A test proves a
      transient failure leaves the index entry intact — this is the acceptance criterion that matters
      most, because its failure mode is invisible to the user until a search comes back short.
- [ ] A genuinely missing path still reports not-found from every site.
- [ ] Tests drive the real entry points, not the helpers.
- [ ] **The two silent-overwrite sites are the priority** — `batch_execute.rs` and `unique_target`.
      A test must prove a denied stat no longer reads as "nothing to overwrite". Failing open into data
      loss outranks a wrong error message.
- [ ] **Read the `exists()` vs `try_exists()` section above before writing any Windows test**, and make
      the deny-effectiveness probe use the same call as the code under test. Getting this wrong is what
      cost CPE-1692 a review round.
- [ ] `transfer.rs:109` is a security guard (CPE-1461 symlink escape). Reason about it explicitly and
      write down the conclusion, even if the conclusion is that it is adequately mitigated.
- [ ] Any skip **announces itself** with `writeln!(std::io::stderr(), ..)` — not `eprintln!`, which
      libtest swallows for passing tests — confirmed visible under plain `cargo test`.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR.
- [ ] Run the sweep again, wider than this ticket's scope, and **state the exact scope you searched**.
      If it comes back clean, that is only ever "clean within X" — name X.

## Notes

Filed by the Foreman from the PR #874 work, 2026-08-12.

Related: **CPE-1678**, **CPE-1687**, **CPE-1692** (the same bug, three times before this), **CPE-1673**
(the taxonomy), and the Evidence Rules in `Ticketing/wiki.md`.
