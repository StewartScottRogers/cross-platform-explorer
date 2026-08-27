---
id: CPE-1944
title: CI never runs `cargo doc`, so three doc defects landed in one ticket with nothing catching them — gate `crates/updater-verify` now, ratchet `crates/server` later
type: task
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

**No workflow in `.github/workflows/` runs `cargo doc` or `rustdoc` at all** — grep-confirmed by PR
#1053's Security Auditor. Over the course of that one ticket, three separate documentation defects
landed and were caught only because a worker chose to run `cargo doc --no-deps` on its own initiative:

1. a doc link to `platforms_not_bound_to_version`, a **deleted symbol**;
2. a module doc still describing the pre-fix mechanism ("the narrow macOS `.app.tar.gz` **naming**
   exception") after the fix had replaced that mechanism entirely;
3. a **public** item linking a **private** one.

The crate in question is `crates/updater-verify` — the one that gates release integrity, and whose
module docs *are* the argument for why the gate is sound. A reviewer who trusts a doc naming a
deleted function is being misled about the mechanism, which is exactly how PR #1053's overclaim
survived a whole review round.

## The gate works, and one crate can adopt it today with zero cleanup

The auditor tested rather than opined. It re-introduced defect 3 and measured:

    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked
      with the defect present : exit 101
        error: public documentation for `platforms_with_mismatched_channel`
               links to private item `channel_fault_of_asset_url`   --> src/lib.rs:669
      on the clean head       : exit 0

So it catches the real defect, at the real line, as a hard failure — and `crates/updater-verify`
**passes it today with no cleanup debt**.

## Why this is not a repo-wide change

`crates/server` currently fails the same command with **528** unresolved-link / private-link errors.
A repo-wide `-D warnings` doc gate is a large cleanup, not a one-line CI addition, and bundling the
two would sink the useful half.

## Acceptance criteria

- [ ] Add `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked` to the existing
      `crates/updater-verify` CI leg, beside its clippy step. It must be **fatal**, like every other
      verification step in this repo.
- [ ] **Red-proof it**: re-introduce one of the three defects above and confirm the job fails naming
      the item and line; confirm the clean head passes. Both directions.
- [ ] Confirm it is cheap. This repo's CI already runs about an hour; `cargo doc --no-deps` on one
      small crate should be seconds against an already-warm target dir. **State the measured runtime.**
- [ ] Record the **528-error `crates/server` backlog** as a separate, later ratchet — do not attempt
      it here, and do not let it block this. If a ratchet is the right shape, it belongs in the
      registry CPE-1934 built (`scripts/ratchet-baselines.mjs`), so the count can only ever shrink.
- [ ] Check the other Rust crates (`cpe-contract`, the sidecar crates, `src-tauri`) — any that pass
      cleanly today should get the gate at the same time, since adopting it costs nothing where there
      is no debt. **Enumerate rather than assume** (CPE-1932).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1053's round-4 Security Auditor, which recommended
exactly this scoping after measuring both halves.

Family: **CPE-1933** (provenance claims in comments — a doc asserting something untested by
construction), **CPE-1929** (guards that cannot go red), **CPE-1934** (the ratchet registry this
crate's backlog would use). All the same thing: **a claim that looks checked and is not.**

Related: **CPE-1923** (the ticket the three defects landed in, PR #1053).
