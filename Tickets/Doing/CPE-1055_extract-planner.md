---
id: CPE-1055
title: "Extract planner — cpe_server::extract_plan (paths, collisions, zip-slip, totals)"
type: feature
component: Backend
priority: medium
status: Doing
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
- [ ] `../evil`, `/abs/path`, `C:\x` (drive-letter) are rejected into `skipped_unsafe`, never planned.
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
