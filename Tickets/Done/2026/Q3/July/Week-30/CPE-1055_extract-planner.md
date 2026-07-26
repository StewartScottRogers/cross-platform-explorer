---
id: CPE-1055
title: "Extract planner — cpe_server::extract_plan (paths, collisions, zip-slip, totals)"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-25
epic: CPE-705
estimate: 3-4h
---

## Summary
Child of CPE-705 (Archive & compression suite). Add a **pure** planner that, given an archive's entry list +
the destination's existing names, computes exactly what an extract would do: normalized output paths,
zip-slip rejections, collisions, directories to create, and totals. Backend-only, `cargo test` on the 3-OS
matrix — no GUI, no user resource, **no new deps** (operates on `ArchiveEntry` vecs; does not open archives).

## Design (buildable)
New module `crates/server/src/extract_plan.rs`, registered with `pub mod extract_plan;` in
`crates/server/src/lib.rs` **immediately after the line `pub mod archive_safety;`**.

Shared input already exists: `archive::ArchiveEntry { name: String, size: u64, is_dir: bool }`.
**REUSE the existing zip-slip guard** `archive::entry_name_is_safe` (crates/server/src/archive.rs ~line 359;
already pure + tested) — do NOT duplicate it. If it isn't `pub`, make it `pub(crate)` and use it (a
one-line visibility change to archive.rs is acceptable; don't reimplement the logic).

```rust
pub struct ExtractPlan {
    pub files: Vec<PlannedEntry>,      // safe entries with resolved dest path + collision flag
    pub skipped_unsafe: Vec<String>,   // raw names rejected as zip-slip
    pub dirs_to_create: Vec<String>,   // deduped, in creation order (parents before children)
    pub file_count: usize,
    pub total_uncompressed: u64,
}
pub struct PlannedEntry { pub archive_name: String, pub dest_rel: String, pub collides: bool, pub size: u64 }

pub fn plan_extract(entries: &[archive::ArchiveEntry], existing_dest: &[String]) -> ExtractPlan
```
Per entry: normalise the inner path (`\`→`/`, strip leading `./`); reject via `entry_name_is_safe`
(`..`/absolute/drive-letter escapes) into `skipped_unsafe`; otherwise compute `dest_rel`, mark `collides`
if `dest_rel` is in `existing_dest`, and collect its parent dirs into `dirs_to_create` (deduped, parents
first). Sum `file_count` (non-dir) and `total_uncompressed`. The total is shaped to feed
`archive_safety::expansion_ratio` for a zip-bomb cross-check (mention it in the doc-comment).

## Acceptance Criteria
- [ ] `../evil`, `/abs/path` are rejected into `skipped_unsafe`, never planned, on all three OSes.
      `C:\x` (drive-letter escape) is rejected too, but **Windows-only**: the reused
      `entry_name_is_safe` guard's absolute-path check is platform-native (`Path::is_absolute()`), so
      on Linux/macOS `C:\x` parses as a plain relative path and is NOT rejected there. The
      Windows-only assertion lives in a `#[cfg(windows)]`-gated test.
- [ ] A dest name already in `existing_dest` is flagged `collides: true`.
- [ ] `dirs_to_create` is deduped and ordered parents-before-children; nested entries derive the right set.
- [ ] `file_count` / `total_uncompressed` sums correct; empty archive → empty plan (no panic).
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default AND
      `--features index` builds; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as the highest-value CPE-705 headless slice (the
safety/conflict core the epic DoD needs). Reuses the existing tested `entry_name_is_safe` guard rather than
duplicating it. Independent module; one-line lib.rs `pub mod` (+ possibly a one-line visibility change to
archive.rs).

2026-07-25 (workshift, Worker) — Implemented `crates/server/src/extract_plan.rs` (`plan_extract` +
`ExtractPlan`/`PlannedEntry`), registered via `pub mod extract_plan;` in `lib.rs` immediately after
`pub mod archive_safety;`. Reused `archive::entry_name_is_safe` — changed its visibility from private to
`pub(crate)` (one-line edit to `archive.rs`, logic untouched) rather than duplicating the zip-slip check.
9 unit tests added covering: unsafe rejection (`../evil`, `/abs/path`, `C:\x`), collisions against
`existing_dest`, dedup + parents-before-children dir ordering (including explicit directory entries),
`file_count`/`total_uncompressed` sums, backslash + leading-`./` normalisation, and the empty-archive
no-panic case. `cargo test -p cpe-server`: 773 passed, 0 failed (incl. the 8 new `extract_plan` tests).
`cargo clippy --all-targets -- -D warnings` clean; `cargo clippy --all-targets --features index -- -D
warnings` clean. No new dependencies.

**Logged assumption:** `entry_name_is_safe`'s absolute-path check (`Path::is_absolute()`) is
platform-native — a `C:\x`-style drive-letter escape is only detected as absolute on Windows; on Linux/
macOS `Path::new("C:/x")` parses as a plain relative path under Unix path semantics, so that specific
input would *not* land in `skipped_unsafe` there (traversal via `..` and POSIX-style `/abs/path` reject
correctly on all three OSes). This is pre-existing behaviour in `archive.rs`'s guard, not introduced by
this ticket, and out of scope to fix here per "reuse, don't duplicate" — flagging for the epic owner
(CPE-705) to consider a follow-up ticket if the 3-OS CI matrix should assert `C:\`-escape rejection on
non-Windows runners too. Verified locally on Windows only (no cargo/Linux runner available in this
worktree); CI will confirm the other two OSes on PR.

Branch `cpe-1055-extract-plan`, PR opened. Ticket left in `Doing/` pending PR review/merge (not moved to
`Done/` by the Worker — that's the Reviewer/Foreman's call per the QA gate).
