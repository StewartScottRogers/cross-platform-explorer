---
id: CPE-1865
title: neither build refuses a stale lockfile, so version drift has no backstop
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-22
closed:
---

## Problem

CLAUDE.md records that `package-lock.json` and `src-tauri/Cargo.lock` are the two version-sync files that
get missed, because **nothing fails when they drift**. CPE-1853 measured exactly what happens, on
throwaway crates with the manifest at `0.2.0` and the lock at `0.1.0`:

| command | result |
|---|---|
| `cargo build` | **exit 0**, and it **silently rewrites** the lock to 0.2.0 |
| `cargo build --locked` | **exit 101** — *cannot update the lock file … because `--locked` was passed* |
| `npm ci` | **exit 0**, "up to date", lock left stale |
| `npm install` | **exit 0**, and it **silently repairs** both fields |

The npm rows are the whole story of how `package-lock.json` sat three releases behind through green CI:
CI's `npm ci` neither fails nor fixes it, a developer's `npm install` fixes it without telling anyone, and
the repair surfaces only as a dirty tree that reads as unrelated noise.

## What CPE-1853 already did, and what is left

CPE-1853 made `scripts/release.ps1` bump all five files atomically, under an exactly-one-match guard, with
a test asserting all five agree. **For `package-lock.json` that script is now the only mechanism** — there
is no build-level backstop at all.

`--locked` would give the Rust half a backstop independent of the release script. That is defence in
depth and worth having. It is also a real behaviour change: `--locked` reds on **any** uncommitted
dependency-graph change, not just a version drift, which is why CPE-1853 recorded the recommendation
rather than taking it.

## Acceptance criteria

- [ ] Decide whether `--locked` goes on the Rust builds, and where — CI only, release only, or everywhere.
      Record the reasoning either way.
- [ ] If taken: measure how often it would have redded CI over recent history before turning it on. A
      backstop that fires on ordinary dependency work will be switched off within a week.
- [ ] Say what, if anything, gives `package-lock.json` a backstop. `npm ci --dry-run` behaves the same as
      `npm ci` (measured), so the honest answer may be "nothing, and the release script's all-five guard is
      it" — which is fine, but should be written down rather than assumed.
- [ ] Check the sidecar and any other Cargo workspace in the tree, not just `src-tauri`. A partial sweep
      presented as complete is this repo's most-repeated defect.
- [ ] If `--locked` is taken, confirm it against the real `tauri build` and the full CI matrix, not a
      throwaway crate. CPE-1853's measurements were single-file crates and it said so.

## Notes

Filed from CPE-1853, whose acceptance criteria required the decision be **recorded**, which it was, with
measurements — but the recommendation landed nowhere actionable until now. Its reviewer flagged the
missing ticket.

Read CPE-1853's Work Log first for the measurement method and the traps in that file; do not re-derive
them.

Related: CPE-1853 (the five-file bump), CPE-1841 (the scoped locators), CPE-1852 (the atomic write).
