---
id: CPE-990
title: "Op-plan model + validator + dry-run (AI copilot)"
type: feature
component: Backend
priority: high
tags: ready
status: Done
created: 2026-07-24
epic: CPE-977
---

## Summary
The pure, filesystem-free structured **operation-plan model** an AI copilot's natural-language instruction
will later compile to — plus its safety **validator** and a **dry-run summary**. No LLM, no I/O: the safe,
inspectable middle of the copilot. Lives in `crates/server/src/op_plan.rs` (crate `cpe-server`).

## Design
- `FileOp` — a **closed, whitelisted** enum: `Move { src, dst }`, `Rename { path, new_name }`,
  `Delete { path }`, `Mkdir { path }`, `Copy { src, dst }`. No free-form/shell op exists by construction, so
  any plan is always inspectable. Serde-derived (`snake_case` tags), plus `Debug/Clone/PartialEq/Eq` and the
  crate's optional `specta::Type` gate, matching `action_macro`.
- `FileOpPlan { ops: Vec<FileOp> }` — the ordered plan (serde + `Default`).
- `PlanLimits { max_ops, root }` — the safety envelope: a scope `root` every path must stay under and a hard
  op cap. Constructors `PlanLimits::new(root, max_ops)` and `PlanLimits::with_root(root)` (uses
  `DEFAULT_MAX_OPS = 1000`).
- `validate(plan, &limits) -> Result<(), Vec<String>>` — returns **all** violations at once: op count over
  cap; any empty/out-of-scope path field; empty-or-non-bare `new_name`.
- `PlanSummary { moves, renames, deletes, mkdirs, copies }` with `total()` + `is_empty()`, produced by
  `summarize(plan)` — the per-kind counts a confirm dialog shows.
- Pure std + serde; **zero** new dependencies (`serde_json` already a dep, used only in a round-trip test).

### `..`-escape / path-scope handling (the load-bearing decision)
Path-scope is **pure and string-based** — no `canonicalize`, no filesystem access (paths in a plan needn't
exist yet). `within_root(root, path)`:
1. `normalize(s)` splits on **both** separators (`/` and `\`) into segments, dropping empty and `.`
   segments; a `..` **pops** the previous segment. If a `..` pops when the stack is empty, the path has
   climbed above the virtual filesystem root — `normalize` returns `ok = false` (an escape).
2. A path is in scope iff its resolved segment list **starts with** the resolved root's segments. So
   `root/a/../b` resolves back inside and passes; `root/../sibling` resolves to `sibling` (root prefix
   gone) and fails; `../up` underflows and fails. Segment-prefix (not raw string-prefix) comparison means
   `/projectX` does **not** count as under `/project`.

Assumptions logged (no user available):
- **Relative paths are not auto-joined to root.** A plan path is compared as-written; a bare relative path
  fails the segment-prefix check. Plans are expected to carry root-anchored paths (the translator will emit
  them from the current folder context). Kept strict rather than guessing an anchor destructively.
- **`new_name` is a bare filename**, not a path: `validate` rejects empty, path separators, and `.`/`..`.
  The ticket asked only for the empty check; the extra guard was added because a rename must not relocate or
  climb the tree (that is what `Move` is for, and `Move` paths are scope-checked). In the copilot's
  safety spirit.
- **Case-sensitive, no Unicode normalisation** in the segment comparison — pure logical string matching,
  consistent cross-platform and deterministic for tests. A real executor still resolves against the OS.
- Malformed root (one whose own `..` underflows) makes every path fail closed.

## Acceptance Criteria
- [x] `FileOp` closed whitelist (Move/Rename/Delete/Mkdir/Copy) with the required derives; serde round-trips.
- [x] `FileOpPlan`, `PlanLimits` (+ sensible constructor), `PlanSummary` (+ `total`/`is_empty`).
- [x] `validate` returns **all** violations: over-cap, out-of-scope (incl. `..` escape), empty path/new_name.
- [x] Path-scope is pure/string-based and robust to `..` climbing out of root.
- [x] `summarize` counts each op kind + total.
- [x] `pub mod op_plan;` added to `crates/server/src/lib.rs` with a doc comment.
- [x] Tests cover clean plan, out-of-scope, `..` escape, over-cap, empty path/new_name, multiple-at-once,
  summarize, and serde round-trip.
- [x] `cargo test op_plan::` green (12 passed).
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `cargo clippy --all-targets --features index -- -D warnings` clean.
- [x] Zero new dependencies.

## Work Log
- 2026-07-24: Read `restore_plan.rs` + `action_macro.rs` for house style (module-doc header, serde +
  `specta` cfg-gate, `PartialEq/Eq` derives, `Summary::total()/is_empty()` pattern, `#[cfg(test)] mod tests`
  with in-memory fixtures). Built `op_plan.rs` aligned to them — not a duplicate; this is the *copilot's*
  intent-level plan type versus `restore_plan`'s filesystem-state diff and `action_macro`'s macro expansion.
- 2026-07-24: Implemented the pure segment-based `within_root`/`normalize` scope check (see Design). Chose
  segment-prefix matching + underflow detection so `..` cannot climb out and text-prefix collisions
  (`/projectX`) don't slip through. Verified: `cargo test op_plan::` = 12 passed; both clippy modes
  (default + `--features index`) clean with `-D warnings`; no new deps.
- 2026-07-24: Ticket → Done; opened PR from branch `cpe-990-op-plan-model`.
