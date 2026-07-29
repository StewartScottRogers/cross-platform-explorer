---
id: CPE-1005
title: Cascade-aware empty-folder finder
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-1002
---

# CPE-1005 — Cascade-aware empty-folder finder

## Summary

A pure cascade-aware empty-folder finder, child of epic CPE-1002 ("File inspection & safety
utilities"). Operates on a caller-supplied directory tree — the filesystem walk that builds that
tree is the adapter's job, out of scope here. No dependencies, no I/O.

New module `crates/server/src/empty_dirs.rs`.

## Design

- `pub struct DirNode { pub path: String, pub file_count: usize, pub children: Vec<DirNode> }` —
  a directory and its immediate file count + subdirectories (derives `Debug, Clone, PartialEq,
  Eq`). A `DirNode::leaf(path, file_count)` convenience constructor covers the common no-children
  case in tests.
- `pub fn cascade_empty(root: &DirNode) -> Vec<String>` — returns the paths of the **topmost**
  cascade-empty directories, in deterministic pre-order.
- **Cascade-empty rule**: a directory is cascade-empty iff it has zero files of its own AND every
  one of its immediate children is (recursively) cascade-empty. A chain of nested empty folders
  collapses to "cascade-empty" once the recursion bottoms out.
- **Topmost-only reporting**: once a node is found cascade-empty, its path is recorded and the
  walk does **not** descend into its children — they're implied by the ancestor being reported.
  Non-cascade-empty nodes recurse into each child so any cascade-empty subtree further down still
  reports itself.
- `root` itself may be reported if the whole tree is cascade-empty.
- Implementation is a straightforward two-function recursive tree walk (`is_cascade_empty` +
  `collect`) — pure std, zero dependencies, zero I/O.
- `pub mod empty_dirs;` added to `lib.rs` with a short doc comment.

## Acceptance Criteria

- [x] `DirNode { path, file_count, children }` derives `Debug, Clone, PartialEq, Eq`.
- [x] `cascade_empty(root) -> Vec<String>` returns only the **topmost** cascade-empty directories
  — a nested-empty child under a reported cascade-empty ancestor is never separately listed.
- [x] Worked example from the ticket (`a` has a file; `b/c` both empty; `d/e/f` all empty down the
  chain; `g` has 0 files but child `g/h` has a file) reports exactly `["b", "d"]` — not `a`, not
  `g`, not any nested path under `b` or `d`.
- [x] A completely empty single node reports itself.
- [x] A node with a file (anywhere in its own count) is not reported, and neither is any ancestor
  that has other non-empty content elsewhere in its subtree.
- [x] A directory with 0 files of its own but a content-bearing descendant anywhere below it is
  correctly excluded (proves the "file anywhere below" propagation, not just immediate children).
- [x] Output order is deterministic (pre-order / input order), asserted directly.
- [x] Pure std, zero new dependencies, zero filesystem I/O.
- [x] `pub mod empty_dirs;` declared in `lib.rs` with a doc comment.
- [x] `cargo test --lib empty_dirs` passes.
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `cargo clippy --all-targets --features index -- -D warnings` clean.

## Work Log

- 2026-07-24 — Built `empty_dirs.rs` end-to-end: `DirNode` (+ `leaf()` convenience constructor)
  and `cascade_empty(root: &DirNode) -> Vec<String>`.
  - **Algorithm**: two small recursive functions. `is_cascade_empty(node)` is `file_count == 0 &&
    children.iter().all(is_cascade_empty)` — a pure bottom-up predicate. `collect(node, out)`
    walks pre-order: if the node is cascade-empty, push its path and **stop** (don't descend —
    its cascade-empty descendants are implied); otherwise recurse into each child so any
    cascade-empty subtree further down still gets reported at its own topmost point.
  - **Assumption**: `path` is treated as an opaque caller-chosen label — never parsed, joined, or
    validated (e.g. as a relative vs. absolute path, forward vs. backslash separators). The ticket
    scoped the real filesystem walk out entirely, so the module makes no assumptions about path
    representation beyond "clone it into the output".
  - **Test fixtures**: hand-built `DirNode` trees exercising exactly the two structural rules the
    ticket called out — (1) topmost-only reporting (`b` and `d` reported, never `b/c`/`d/e`/`d/e/f`)
    and (2) "a file anywhere below" disqualifies every ancestor up the chain, not just the
    immediate parent (`g/h/i` having a file disqualifies both `g/h` and `g`). Also covers: single
    empty node, deep empty chain, independent siblings (order proof), whole-tree-empty reporting
    `root` itself, and explicit pre-order determinism with non-alphabetical input order.
  - Verified (PowerShell, `crates/server`, `$env:PATH += ";$env:USERPROFILE\.cargo\bin"`):
    `cargo test --lib empty_dirs` → 8/8 passed; `cargo clippy --all-targets -- -D warnings` clean;
    `cargo clippy --all-targets --features index -- -D warnings` clean. No clippy fixes needed.
  - Status → Done; ACs checked; moving to
    `Tickets/Done/2026/Q3/July/Week-30/CPE-1005_empty-folder-finder.md`.
