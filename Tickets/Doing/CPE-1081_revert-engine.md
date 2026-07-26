---
id: CPE-1081
title: "Revert engine — cpe_server::revert_engine (surgical real-fs restore execution)"
type: feature
component: Backend
priority: high
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-732
estimate: 2-3h
---

## Summary
Child of CPE-732 (Checkpoint & rollback). Execute a MINIMAL restore plan (Create/Overwrite/Delete) against a
real directory — distinct from CPE-735's `snapshot_capture::restore` which only writes a whole manifest and
never deletes or considers current state. Backend-only, `cargo test` on the 3-OS matrix (real tempdir
round-trip) — no GUI, no user resource, no new deps. Reuses `restore_plan::RestoreAction`, `snapshot_capture::
scan_dir`, `checksum`; does NOT modify them. Parallel-independent.

## Design (buildable)
New module `crates/server/src/revert_engine.rs`, registered `pub mod revert_engine;` in
`crates/server/src/lib.rs` **immediately after `pub mod snapshot_capture;`**. Read `snapshot_capture.rs` for
how blobs are stored (`store_dir/blobs/<hash>`) + `scan_dir`, and `restore_plan.rs` for `RestoreAction`/its
kind (Create/Overwrite/Delete) + `Snapshot`.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RestoreReport { pub applied: usize, pub skipped: Vec<(String /*path*/, String /*why*/)> }
pub fn execute_restore(plan: &[RestoreAction], dest_root: &str, store_dir: &str, checkpoint: &Snapshot)
    -> RestoreReport;
```
- **Create/Overwrite**: `create_dir_all(parent)` then copy `store_dir/blobs/<checkpoint[path].hash>` to the
  target. **Delete**: `remove_file`, applied **deepest-first** (reverse `/`-segment depth) so dirs empty
  cleanly.
- Each failing op is **skipped and recorded in `skipped`, never fatal** (mirror the `list_dir`/
  `snapshot_capture` skip-on-error guardrail). `applied` counts successes.
- **Path safety**: reject any plan `rel` that escapes `dest_root` (`..`/absolute/drive) via a `/`-segment
  containment guard; build the real target with `Path::join` over the split segments, NOT string concat.

## ⚠ Cross-OS
Build the target via `Path::join` over `/`-split segments (portable), but the escape guard + depth ordering
operate on normalized `/`-segment strings (no `starts_with`). **`#[cfg(unix)]`-gate** any chmod/permission
test — prefer a portable missing-blob simulation (point at a nonexistent blob hash) so the Windows CI leg
stays green. No unchecked arithmetic; no recursion (iterate the plan).

## Acceptance Criteria
- [ ] Real tempdir round-trip: build a checkpoint dir → mutate (edit + add + delete) → `plan_restore` →
      `execute_restore` → `scan_dir` again equals the checkpoint snapshot (byte-for-byte content restored).
- [ ] A missing-blob op lands in `skipped` (with a reason) while the rest apply (`applied` counts them);
      never panics.
- [ ] A `../escape` (or absolute) plan path is REFUSED (skipped with reason), never written outside dest_root.
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean default AND
      `--features index`; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as CPE-732's surgical execution layer (distinct from
CPE-735's whole-manifest restore). Parallel-independent. Distinct lib.rs anchor. Uses a real tempdir like the
snapshot_capture tests (process-unique dir + cleanup).

2026-07-25 (workshift, overnight Worker) — Built `crates/server/src/revert_engine.rs` per the design:
`RestoreReport {applied, skipped}` + `execute_restore(plan, dest_root, store_dir, checkpoint)`. Registered
`pub mod revert_engine;` immediately after `pub mod snapshot_capture;` in `lib.rs`. Std-only, no new deps;
`restore_plan`/`snapshot_capture` left untouched (import-only reuse).

Confirmed blob layout from `snapshot_capture.rs` module doc + code: `store_dir/blobs/<hash>`, one file per
content hash (`blobs_dir()` = `store_dir.join("blobs")`), matching the ticket's assumption exactly.

Path-safety guard: `safe_segments()` splits the plan's `/`-joined `rel` path and rejects (skip, not panic)
on: empty string, `Path::is_absolute()`, any empty/`.`/`..` segment, or a segment containing `:`/`\` (blocks
a Windows drive-letter or backslash segment slipping through). Real target rebuilt via `Path::join` over the
validated segments in `safe_target()` — no string concatenation, no `starts_with`.

Deepest-first deletes: plan partitioned once into writes vs. deletes; deletes sorted by
`Reverse(segment_depth(path))` (segment count via `rel.split('/').count()`) before applying, so nested files
are removed before anything above them — matches `restore_plan`'s execution-note doc comment. No recursion
anywhere; the plan is iterated linearly.

Tests (5, all real-tempdir, process-unique `cpe-revert-<tag>-<pid>-<seq>` dirs, cleaned up): full round-trip
(edit+delete+add mutation → `plan_restore` → `execute_restore` → `scan_dir` equals checkpoint byte-for-byte);
missing-blob skip (portable — a checkpoint entry whose blob file was never written, no OS-specific
permission tricks, so the Windows CI leg stays green) with the rest of the plan still applying; an
`../escape.txt` plan path refused with nothing written outside `dest_root`; an absolute plan path
(`/etc/passwd` unix / `C:/evil.txt` windows) refused; deepest-first delete ordering verified directly.

Assumption: the ticket's pseudo-signature omits an explicit escape-guard return type — implemented as
`Result<Vec<&str>, String>` (segments) / `Result<PathBuf, String>` (target) that `apply_write`/`apply_delete`
turn into a `skipped` entry via `?` + a `match`, keeping every failure path (path-safety, missing blob, I/O
error) going through the exact same skip-not-panic funnel.

Verify: `cargo test` (943 passed, 0 failed, incl. the 5 new) green. `cargo clippy --all-targets -- -D
warnings` clean; `cargo clippy --all-targets --features index -- -D warnings` clean (one
`needless_borrows_for_generic_args` lint fixed in the test helpers along the way). No new deps —
`Cargo.toml`/`Cargo.lock` untouched. Opened PR from branch `cpe-1081-revert-engine`.
