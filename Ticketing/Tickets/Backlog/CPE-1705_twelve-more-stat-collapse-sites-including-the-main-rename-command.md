---
id: CPE-1705
title: Twelve more stat-collapse sites — including the main rename command, which can silently clobber a file
type: bug
priority: High
status: Backlog
tags: ready
estimate: L
created: 2026-08-13
closed:
---

## Problem

The **sixth** round of this bug class (CPE-1678 → 1687 → 1692 → 1696 → this), and the first where the
sweep behind it was genuinely exhaustive: **all 341 tracked `.rs` files, nothing excluded** — every file
under `tests/`, `examples/`, `benches/`, `src/bin/`, `build.rs`, and `sidecar/` in full, twelve patterns,
brace-matched to separate production code from `#[cfg(test)]`. Found by the CPE-1696 worker, which
reported rather than fixed them, following the precedent CPE-1692 set. That was the right call — but it
means **these are unfixed on `main` right now.**

Three are at the same silent-overwrite severity as CPE-1696's priority sites. One of them is the main
rename command.

### Class A — refuse-to-overwrite via `.exists()`, then `fs::rename`, which replaces silently

The shape: the code checks `.exists()` to refuse clobbering an existing file, then calls `fs::rename`,
which **replaces the destination silently on both Windows and Unix**. A denied or failed stat makes
`.exists()` return `false`, the guard passes, and the rename destroys the file that was there.

- **`src-tauri/src/lib.rs:1786` — the main rename command.** This is the most-used operation in a file
  explorer. Rename a file to a name that already exists in a folder you cannot fully stat, and the
  existing file is gone with no warning and no error.
- `src-tauri/src/lib.rs:3317`
- `crates/server/src/copilot.rs:226`, `:255`
- `crates/server/src/organize_apply.rs:85`
- `crates/server/src/folder_template.rs:173`
- `crates/server/src/split_join.rs:111`, `:118`, `:314`
- `src-tauri/src/lib.rs:1869`, `:2102` — both carrying a "Never clobber" comment
- `src-tauri/src/lib.rs:156`

### Class B — unique-name loop, the exact `unique_target` shape CPE-1696 fixed

- **`crates/server/src/batch_media.rs:2054`** — the **planner** feeding the very executor CPE-1696
  hardened. Fixing the executor and leaving the planner means the plan is built on a false premise.
- `crates/server/src/snapshot_capture.rs:444`

### Class C — a security guard, same class as `transfer.rs:109`

- `crates/server/src/batch_media.rs:1736`

## Read this before writing a Windows test — it changes what is provable

Measured by the CPE-1696 worker and recorded on `fsutil::deny_stat_of`:

- The **minimal** Windows deny that makes `try_exists` fail is **`S` (SYNCHRONIZE)**. `RA`, `REA`, `RD`
  and `RC` do nothing.
- But `fs::write` and `fs::copy` **also request SYNCHRONIZE**. So **every deny that hides a file also
  protects it.**

Consequence: on Windows an ACL test can prove the code *refuses*, but can never demonstrate *byte loss*.
Worse, a bare `expect_err` **passes vacuously** — the neutralised code still errors, just with
"Access is denied. (os error 5)" coming from the write rather than from the guard. Assert on which error,
or test the pure classifier. This is why CPE-1696's classifiers are the load-bearing tests.

## Scope

The twelve sites above. **Deliberately excluded, do not re-open:**

- The `.is_dir()` type-check family — CPE-1692 made an explicit documented decision.
- The 16 `.ok()?` scanner sites — CLAUDE.md's stated "skip entries we cannot read" contract for `list_dir`.
- Four `create_new`-backed sites, already atomically mitigated.

## Acceptance criteria

- [ ] **Start with `lib.rs:1786`.** It is the highest-traffic path and the clearest data-loss risk. A test
      must prove a rename onto an existing name **refuses** when the stat fails, and still proceeds when
      the destination is genuinely absent. Both directions — a fix that refuses everything is as broken as
      one that overwrites.
- [ ] `batch_media.rs:2054` is fixed in the same PR as, or before, anything that consumes its plans —
      CPE-1696 hardened the executor and this is the planner.
- [ ] Every Class A site distinguishes "the destination is absent" from "I could not tell", and never
      renames on the second. Reuse `dispatch::classify_path_error`'s taxonomy rather than re-deriving one.
- [ ] Consider whether `fs::rename`'s silent-replace semantics warrant a shared helper rather than twelve
      independent guards. Twelve copies of the same check is how the thirteenth gets missed. Decide and
      record.
- [ ] A genuinely missing path still behaves correctly at every site — the honest case must not regress.
- [ ] Tests drive the real entry points, not the helpers.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per
      the Evidence Rules in `Ticketing/wiki.md`. Watch for the vacuous `expect_err` described above.
- [ ] Re-run the sweep at the same scope and state it. If it comes back clean this time, that is worth
      saying explicitly — it would be the first time in six rounds.

## Notes

Filed by the Foreman from the PR #889 (CPE-1696) sweep, 2026-08-13. The worker flagged it unprompted:
*"the sweep's Class A/B/C findings are unfixed silent-overwrite bugs on `main` right now, including the
main rename command — I'd recommend filing that follow-up before this sprint's queue moves on."*

Related: **CPE-1678**, **CPE-1687**, **CPE-1692**, **CPE-1696** (the same bug, four times before this),
**CPE-1673** (the taxonomy), **CPE-1461** (the guard class C belongs to).
