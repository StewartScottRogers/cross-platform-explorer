---
id: CPE-1690
title: cpe-mdns's 17 tests have never run in CI, and its test code has never been linted
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

`crates/mdns` is the only crate in `crates/` that `.github/workflows/ci.yml` does not name:

```
crates/          -> contract ftp mdns net security server sftp updater-verify vfs webdav
named in ci.yml  -> contract ftp     net security server sftp updater-verify vfs webdav
```

(`crates/s3` is added by PR #867, so it is covered.)

### What is and is not true — stated precisely, because the distinction is the whole ticket

Surfaced by the PR #867 reviewer, whose phrasing was *"`crates/mdns` is never compiled by CI"*. **That is
too strong, and I checked before filing.** `src-tauri/Cargo.toml:73` declares `cpe-mdns = { path =
"../crates/mdns" }`, so the Backend job compiles the crate's **library** on all three OSes. A compile error
in `lib.rs` would be caught.

What is genuinely missing:

- **Its 17 tests have never run.** `cargo test` in `src-tauri` does not run a path dependency's unit tests.
  Nothing anywhere executes them.
- **Its test code has never been linted.** Every sibling crate gets `cargo clippy --all-targets -D
  warnings`, which covers `#[cfg(test)]` code. `mdns` gets whatever `src-tauri`'s build implies, which
  excludes its tests entirely.

So the crate is *built* but not *verified*. That is a narrower hole than "not compiled" — and still a real
one, because 17 tests that cannot fail are 17 tests that are not evidence of anything.

### Why it matters here specifically

`crates/mdns` is the network-discovery crate: it browses `_nfs._tcp.local.`, `_smb._tcp.local.` and friends
and maps them to `ShareProtocol` values the UI shows. That is protocol-parsing code over untrusted
multicast input from the local network — precisely the kind of code where a silent regression is both
plausible and invisible. Its tests were written to catch that and have never once been given the chance.

## Scope

`.github/workflows/ci.yml` — add an `mdns — clippy + test` step alongside its siblings in the `Server
crates` job, matching their exact shape (both feature modes if the crate has features).

**Then look at what turns red**, and do not assume the answer is nothing. Tests that have never executed in
CI have never faced Linux or macOS, and this crate does network-interface enumeration and multicast, which
is exactly where per-OS behaviour differs. If they fail there, that is the ticket delivering its value, not
a setback — fix them or mark them clearly, but do not weaken an assertion to get green.

While in the file, add a guard so this cannot recur: a check that every directory under `crates/` appears in
the workflow. A comment asking people to remember is not a guard.

## Acceptance criteria

- [ ] `mdns — clippy + test` runs in CI on all three OSes, and the **step-level** conclusion is checked
      (a green umbrella job does not prove the step ran — the PR #867 reviewer had to read step conclusions
      to confirm the `s3` step was real).
- [ ] All 17 tests pass on Linux, macOS and Windows — or any that cannot are explicitly and visibly skipped
      with the reason, announced rather than silently absent (see CPE-1678: a skip that says nothing is
      indistinguishable from a pass).
- [ ] `cargo clippy --all-targets -D warnings` is clean for the crate, test code included.
- [ ] A guard fails CI when a directory under `crates/` is not covered by the workflow, and breaking it
      (removing a crate's step) turns that guard red — proven with the real output.

## Notes

Filed by the Foreman from the PR #867 review, 2026-08-12. The reviewer surfaced it as an out-of-scope
observation; the "compiled but unverified" correction is mine, made by checking `src-tauri/Cargo.toml`
before writing the ticket rather than repeating the claim.

Related: the Evidence Rules in `Ticketing/wiki.md` — this is the same family as CPE-1680 and CPE-1678: a
check that looks like it is happening and is not.
