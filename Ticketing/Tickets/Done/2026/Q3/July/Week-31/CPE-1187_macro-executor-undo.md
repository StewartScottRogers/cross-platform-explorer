---
id: CPE-1187
title: "Macro executor + undo model (headless) with scope guard"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-31
epic: CPE-739
---

## Summary
Part of CPE-739. The macro model (`action_macro.rs` `plan()`), library (`macro_library.rs`), and store
(`macro_store.rs`) exist, but **nothing runs a macro** — `plan()` emits `PlannedOp`s that are never applied.
Build the pure resolution + inverse (undo) + scope-guard layer so a multi-step macro run is reversible and
can't escape the working root. (Actual disk writes happen in the CPE-1188 command.)

## Build
- New `crates/server/src/macro_run.rs` (+ `mod` line in `crates/server/src/lib.rs`): given `(ActionMacro,
  inputs)` or a `Vec<PlannedOp>`, produce a **resolved, collision-safe** ordered op list plus a per-op
  **inverse** record so the run is reversible; and a **scope check** (reuse `op_plan::within_root`-style logic)
  rejecting any resolved dest outside the working root. Pure logic — no disk writes here.
- Deterministic collision-safe naming; imported macros never auto-run (that gate lives in the command/UI).

## Acceptance Criteria
- [x] `cargo test -p cpe-server`: inverse of a rename/move/tag/convert sequence round-trips (apply-then-invert
      restores state, tested at the resolution level); out-of-root dest rejected; collision-safe naming
      deterministic.
- [x] `cargo clippy --all-targets -D warnings` clean (both feature modes).

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-739). Backend phase; built with CPE-1188 by one worker.
- 2026-07-31 — Built `crates/server/src/macro_run.rs` (new module, `mod` added in `crates/server/src/lib.rs`):
  `resolve(&ActionMacro, &[String], root: &str) -> Result<ResolvedRun, Vec<String>>`. Threads each input's
  path through its steps (a rename/move/convert changes `current`, a tag doesn't), reusing
  `action_macro::plan`'s already-resolved per-step `detail` so template/token semantics stay exactly as
  before. Collision-safe naming: a `HashSet<String>` seeded with every original input, deterministic
  `-2`/`-3`… suffix on any destination collision (mirrors `batch_media::plan`'s guard) — so a rename never
  silently overwrites another selected file, and two inputs planned to the same target are disambiguated in
  input order. Scope guard: every resolved rename/move/convert destination is checked with
  `op_plan::within_root` (reused, not duplicated), reporting **every** violation — this also catches a
  rename template whose literal text (not just an unknown token) tries to `../` climb out of root, since
  the check runs on the resolved path, not the raw template. `ResolvedRun{ops, inverses}`: `inverses` is
  same-length/order as `ops`; applying them **in reverse** undoes the run (`rename`/`move`/`convert` reuse
  their forward `kind`, a tag step's inverse is `"untag"`). 17 new tests: scope-guard (move dest, `..`
  escape via dest, `..` escape via rename-template literal text, in-root accepted, invalid macro rejected,
  empty inputs), collision-safe naming (colliding renames, colliding moves, rename never overwrites an
  untouched selected input), Windows paths + dotfiles, multi-step chaining (a move sees the RENAMED path,
  a tag sees the final path), and a pure in-memory simulation (`SimState`: existing-paths + per-path tags,
  entirely at the resolution level, no disk/tag-store I/O) proving apply-then-invert-in-reverse round-trips
  for rename-alone / move-alone / tag-alone / convert-alone / a full 4-step chained sequence. Documented
  scope limit: the tag/untag inverse pair assumes the label wasn't already on the path before the run (a
  byte-exact prior-`TagEntry` snapshot needs real I/O, which is CPE-1188's layer, not this pure module's).
  `cargo test -p cpe-server`: 1131/1131 passed (macro_run: 17/17). `cargo clippy --all-targets -D warnings`
  clean default + `--features index`. No new dependencies.
