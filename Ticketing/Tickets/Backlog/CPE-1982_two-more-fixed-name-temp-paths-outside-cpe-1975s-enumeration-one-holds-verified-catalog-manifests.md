---
id: CPE-1982
title: Two more fixed-name `temp_dir()` paths outside CPE-1975's enumeration — and one of them holds **verified catalog manifests**
type: bug
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-28
---

## Summary

Found by **PR #1097**'s Reviewer (CPE-1975) while checking that ticket's three-site enumeration. Two more
fixed-name temp paths of the same class sit outside it:

- **`src-tauri/src/lib.rs:10167`** — `temp_dir().join("cpe-ai-console-catalog")`. **This one holds
  verified catalog manifests.**
- **`src-tauri/src/lib.rs:11911`** — `temp_dir().join("cpe-sidecar-storage")`.

Both are **`app_data_dir()` fallbacks**, so they are rarely reached — which is exactly why nobody looked
at them, and why they are worth a ticket rather than a note.

**Both are higher-value targets than the trace log CPE-1975 was filed for.** That ticket's headline
attack — the session-daemon port file as a control channel — turned out **not to be reachable**
(`discover_or_spawn` has zero callers; production learns the port from the child's stdout). What was
live there was a *write* primitive into an attacker's directory and a *delete* primitive. Here the
directory holds **content the app has already decided to trust**.

## The class, and the primitive that defines it

`create_dir_all` is the primitive **CPE-1952** established will **follow a junction/symlink into an
attacker-chosen directory**. A fixed, untimestamped name makes the target enumerable; `cpe-swarm-<millis>`
was at least guessable-within-a-window, a constant is not even that.

**Threat model — state both halves, do not collapse them** (the correction CPE-1964 carries). On
**Windows**, `std::env::temp_dir()` resolves to the **per-user** `%LOCALAPPDATA%\Temp`, so the Windows
attack needs a same-user process. *"Predictable path in a shared namespace"* is fully true of **Unix
`/tmp`**. Both halves are real.

## What this needs

- [ ] **Reproduce first, both platforms, asserting on the filesystem** — where the bytes land — never on
      a returned verdict. Junction on Windows (`junction::create`, no admin); symlink on **real ext4**,
      not `/mnt/z`, and note `/tmp` on WSL is **tmpfs**: override `TMPDIR`.
- [ ] **Then ask what the consequence actually is, and accept the answer.** For
      `cpe-ai-console-catalog`, the question that decides this ticket's priority is whether a redirected
      directory can get **attacker-chosen manifest bytes treated as verified** — or whether verification
      happens before anything reaches this path, in which case the exposure is a write/delete primitive
      like CPE-1975's and should be reported as such. **CPE-1975's most valuable finding was that its own
      headline was not reachable; do not manufacture a consequence you could not reach.**
- [ ] **Use the primitive that already exists.** CPE-1975 landed a hardened creator in
      `sidecar/ai-console/src/console_temp_dir.rs` — `create_dir`, then `symlink_metadata().is_dir()` on
      `AlreadyExists` (a rendezvous must tolerate already existing, so CPE-1964's pure exclusive create
      does not transfer), plus a leaf-link refusal and **no `exists()` pre-check**. These two sites are in
      `src-tauri`, not a sidecar — so decide where the shared version lives and **do not grow a third
      copy** (CPE-1950: where duplication is removable, remove it).
- [ ] **Keep a sensitivity control** — with the fix disabled the redirect must succeed — as a **normal CI
      test on all three OSes, not `#[ignore]`d**, planting at the **real** path. A control that returns
      green because it could not plant its link proves nothing, **invisibly** (PR #1075 lost that leg
      twice); **panic** instead.
- [ ] **Run the CPE-1929 sabotage pair on every refusal added and write both numbers at the site**,
      naming the platform. CPE-1975's eight runs were all Windows-only and said so; a pair can be **split
      across platforms** (green on Windows, red on Linux, each fixture unconstructible on the other side),
      so a Windows-only pair does not settle Linux.
- [ ] **Enumerate rather than recall** (CPE-1932). This ticket exists because a three-site list was
      complete for the file it was derived from and not for the tree. Re-derive every `temp_dir()` site
      with `productionCode` / `productionRustFiles` (`src/lib/rustProductionSources.ts`, lifted to one copy
      by CPE-1975) and report the full list with a verdict each — **including the ones that are fine**.
- [ ] Decide what to do with any existing directories using CPE-1964's five-condition fail-closed shape
      if a sweep is warranted, and obey **CPE-1972**: *an absence of information must never license a
      delete.* A planted directory should be **refused, not removed**.

## Notes

Filed 2026-08-28 by the sprint Foreman from PR #1097's Reviewer (CPE-1975), which found these while
verifying that ticket's enumeration rather than accepting it.

Related: **CPE-1975** (PR #1097 — the same class, the hardened primitive, the controls and the CPE-1929
pairs), **CPE-1964** (PR #1086 — the exclusive-create argument and the shadowed guard it deleted),
**CPE-1952** (`create_dir_all` follows a link; delete the seam where that is available), **CPE-1929**
(shadowed guards), **CPE-1972** (an absence of information must never license a delete), **CPE-1950**
(remove removable duplication), **CPE-1932** (enumerate, don't recall).
