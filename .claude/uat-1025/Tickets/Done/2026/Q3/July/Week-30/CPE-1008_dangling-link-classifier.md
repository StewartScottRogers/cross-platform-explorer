---
id: CPE-1008
title: Dangling + cyclic symlink classifier
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-1002
---

# CPE-1008 — Dangling + cyclic symlink classifier

## Summary

A pure broken/dangling + cyclic symlink classifier, child of epic CPE-1002 ("File inspection &
safety utilities"). Operates on a caller-supplied list of symlink records — the real filesystem
walk, `read_link`, and target-existence check are the adapter's job, entirely out of scope here.
No dependencies, no I/O, and deliberately **no real symlinks created or touched** (Windows symlink
creation needs elevated privileges and would be flaky/unsafe in CI).

New module `crates/server/src/dangling_links.rs`.

## Design

- `pub struct LinkEntry { pub path: String, pub target: String, pub target_exists: bool }` — one
  symlink: its own path, the path it points at (already resolved/normalised by the caller), and
  whether that target currently exists on disk (caller-determined). Derives `Debug, Clone,
  PartialEq, Eq`.
- `pub enum DanglingReason { Missing, Cyclic }` (`Debug, Clone, Copy, PartialEq, Eq`).
- `pub struct DanglingLink { pub path: String, pub reason: DanglingReason }` (`Debug, Clone,
  PartialEq, Eq`).
- `pub fn scan_dangling(links: &[LinkEntry]) -> Vec<DanglingLink>` — classifies each link:
  - **Cyclic** (takes precedence over Missing): follow the target chain within the supplied
    `links` (`link.target` → the `LinkEntry` whose `path` equals that target → its own `target` →
    …). If the walk revisits a path already seen in that walk (a loop, including a direct
    self-loop `A → A`), the link is `Cyclic`.
  - **Missing**: not cyclic AND `target_exists == false`.
  - Otherwise (target exists and the chain never cycles) → not reported at all.
  - Reported in deterministic input order, one entry per dangling link.
- **Cycle-walk algorithm**: build a `path -> &LinkEntry` lookup map once. For each link, walk
  forward from it with a `HashSet<&str>` seeded with the link's own path. At each step, look at
  the current node's `target`; if that path is already in the seen-set, it's a cycle → `true`.
  Otherwise, if the target matches another supplied `LinkEntry`'s path, add it to `seen` and
  continue from there; if it matches no supplied link, the chain runs off the known set and the
  walk terminates → `false` (not cyclic). Bounded: `seen` can only grow up to `links.len() + 1`
  entries before the loop must terminate one way or the other, so there is no risk of spinning
  forever even on a real cycle.
- Pure std (`std::collections::{HashMap, HashSet}`), zero new dependencies, zero I/O.
- `pub mod dangling_links;` added to `lib.rs` with a short doc comment.

## Acceptance Criteria

- [x] `LinkEntry { path, target, target_exists }` derives `Debug, Clone, PartialEq, Eq`.
- [x] `DanglingReason { Missing, Cyclic }` derives `Debug, Clone, Copy, PartialEq, Eq`.
- [x] `DanglingLink { path, reason }` derives `Debug, Clone, PartialEq, Eq`.
- [x] A healthy link (`target_exists: true`, chain never cycles) is not reported.
- [x] A broken link (`target_exists: false`, target not among the supplied links) is reported
  `Missing`.
- [x] A 2-cycle (`A → B`, `B → A`, both `target_exists: true`) reports **both** as `Cyclic`.
- [x] A self-loop (`A → A`) is reported `Cyclic`.
- [x] A 3-cycle (`A → B → C → A`) reports all three as `Cyclic`.
- [x] Cyclic precedence: a link in a cycle is `Cyclic` even when some `target_exists` flag along
  the chain is `false`.
- [x] A chain `A → B → (broken)`: `B` (whose target isn't among the supplied links and whose
  `target_exists` is `false`) is `Missing`; `A` is not cyclic (the chain terminates rather than
  looping) and not reported (its own `target_exists` is `true`) — both facts asserted.
- [x] Output order is deterministic (input order) with multiple danglers present; empty input →
  empty output.
- [x] Pure std, zero new dependencies, zero filesystem I/O — no real symlinks created or read
  anywhere in the module or its tests.
- [x] `pub mod dangling_links;` declared in `lib.rs` with a doc comment.
- [x] `cargo test --lib dangling_links` passes.
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `cargo clippy --all-targets --features index -- -D warnings` clean.

## Work Log

- 2026-07-24 — Built `dangling_links.rs` end-to-end: `LinkEntry`, `DanglingReason`, `DanglingLink`,
  and `scan_dangling(links: &[LinkEntry]) -> Vec<DanglingLink>`.
  - **Cycle-walk approach**: a `path -> &LinkEntry` `HashMap` built once from the input slice, then
    for each link a forward walk (`is_cyclic`) seeded with a `HashSet` containing the link's own
    path. Each step follows the current node's `target`: if that path is already in the seen-set,
    return `true` (cyclic); if it resolves to another supplied `LinkEntry`, insert it into `seen`
    and continue; if it resolves to nothing in the supplied set, return `false` (chain runs off the
    known links — not cyclic, since a link into the unknown is a normal — possibly missing —
    terminal target, not a loop). The seen-set can only grow to at most `links.len() + 1` entries
    before one of those two exits fires, so the walk is bounded and can never spin forever even on
    a genuine cycle.
  - **Precedence rule**: `scan_dangling` checks `is_cyclic` first and only falls through to the
    `target_exists == false` → `Missing` check if the walk was not cyclic, so a link that's both
    "in a loop" and "flagged not-existing by the caller" is always reported `Cyclic` — the cycle
    check never even inspects `target_exists`.
  - **Note on chains into a cycle**: a link that isn't itself part of a loop but whose target chain
    flows *into* one downstream (e.g. `A → B` where `B`/`C` are a 2-cycle) is also flagged `Cyclic`
    under this walk, because the per-walk seen-set accumulates every path visited from that
    starting link, not just a return to the exact start. This is a defensible reading of "revisits
    a path already seen in the walk" (and arguably correct: resolving `A` requires resolving `B`,
    which never terminates), though the ticket's required test list doesn't exercise this exact
    shape, so it's called out here as an assumption rather than a directly-asserted case.
  - **Assumption**: `path`/`target` are opaque caller-chosen string labels — never parsed, joined,
    or path-normalised here (mirrors `empty_dirs`/`organize`'s house style for caller-supplied
    data). The real filesystem walk, `read_link` resolution, and target-existence check are
    entirely the adapter's responsibility and out of scope.
  - **Test fixtures**: hand-built `LinkEntry` records (no real filesystem symlinks anywhere —
    Windows symlink creation needs elevated privileges and would be unsafe/flaky in CI). Covers:
    healthy link (not reported), broken link (`Missing`), 2-cycle (both `Cyclic`), self-loop
    (`Cyclic`), 3-cycle (all three `Cyclic`), cyclic-precedence-over-false-target_exists, a
    chain-into-a-broken-link (only the broken leg flagged, explicitly asserting the healthy leg is
    absent), deterministic multi-dangler ordering, and empty input.
  - Verified (PowerShell, `crates/server`, `$env:PATH += ";$env:USERPROFILE\.cargo\bin"`):
    `cargo test --lib dangling_links` → 9/9 passed; `cargo clippy --all-targets -- -D warnings`
    clean; `cargo clippy --all-targets --features index -- -D warnings` clean. No clippy fixes
    needed.
  - Status → Done; ACs checked; moving to
    `Tickets/Done/2026/Q3/July/Week-30/CPE-1008_dangling-link-classifier.md`.
