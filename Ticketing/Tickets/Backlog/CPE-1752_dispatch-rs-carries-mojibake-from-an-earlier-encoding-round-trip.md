---
id: CPE-1752
title: dispatch.rs carries mojibake from an earlier encoding round-trip
type: bug
priority: Low
status: Backlog
tags: ready
estimate: XS
created: 2026-08-15
closed:
---

## Problem

Spotted by the PR #906 (CPE-1733) round-4 reviewer while verifying that branch's own file integrity, and
confirmed on `main`: `crates/server/src/dispatch.rs` contains **18** occurrences of UTF-8-read-as-CP1252
mojibake. Every one is an em-dash that became `â€"`:

```
dispatch.rs:3  //! ... drives over a socket in CPE-820 â€" here with **no
dispatch.rs:10 //! parent-directory traversal â€" "we don't know" must never be reported as "it isn't there")
dispatch.rs:45 /// structurally â€" CPE-1659, first applied to `list_dir` only)
```

It is **pre-existing on `main`** and untouched by that branch. A tree-wide scan of `crates/server/src/` and
`src-tauri/src/` found no other affected file, so this is a single-file artifact of one bad round-trip, not
a systemic encoding problem.

## Why it is worth a ticket rather than a drive-by

It is only comments, so nothing misbehaves — but the damage is invisible in a normal diff view and the file
still compiles, which is exactly why it survived. This is the same failure that a CPE-1733 worker hit live
on 2026-08-14: a PowerShell `Get-Content`/`Set-Content` round-trip silently adds a BOM and reinterprets
UTF-8 as CP1252, mojibaking every non-ASCII character in the file. That worker caught it only because
`git diff --numstat` read 495/159 instead of the expected ~343/7.

`dispatch.rs` is the `ServerCtx` seam's own doc surface — the file a reader goes to in order to understand
the contract — so garbled prose there is worse than average.

## Acceptance criteria

- [ ] The 18 sequences are repaired to the em-dashes they were, byte-exactly. Use `iconv`/`sed`/an editor
      tool — **not** a PowerShell text round-trip, which is what caused this.
- [ ] The file is `valid-utf8`, has **no BOM**, and no CR bytes beyond whatever `core.autocrlf` produces in
      the working tree (check the git blob, not the checkout).
- [ ] `git diff --numstat` shows a change proportional to 18 single-character repairs — a wildly inflated
      count means the whole file was re-encoded again.
- [ ] A tree-wide scan for the same signatures (`â€`, `Ã`, `Â `, `ï»¿`) reports no remaining hits under
      `crates/`, `src-tauri/src/`, `src/`, and `docs/`; anything found outside `dispatch.rs` gets recorded
      here rather than silently fixed.
- [ ] `cargo build` and `cargo clippy --all-targets -- -D warnings` still clean in both feature modes (a
      comment-only change, so this is a guard against an accidental wider edit).

## Notes

Worth considering as a follow-up rather than here: a cheap CI guard that fails on these byte signatures in
tracked text files would make the whole class impossible to reintroduce. File it separately if you agree.
