---
id: CPE-1057
title: "Archive-vs-folder diff — cpe_server::archive_diff (what's new in this zip)"
type: feature
component: Backend
priority: low
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-705
estimate: 2-3h
---

## Summary
Child of CPE-705 (Archive & compression suite). Add a **pure** diff that classifies an archive's entries
against a destination folder's listing — "what's in this zip I don't already have" before extracting.
Backend-only, `cargo test` on the 3-OS matrix — no GUI, no user resource, **no new deps**. Filed to Backlog:
independent of the other CPE-705 slices, dispatched after the first wave to keep the merge queue drained.

## Design (buildable)
New module `crates/server/src/archive_diff.rs`, registered with `pub mod archive_diff;` in
`crates/server/src/lib.rs` **immediately after the line `pub mod simhash;`** (or another distinct, unused
anchor — pick one not taken by a sibling ticket).

```rust
pub struct FolderEntry { pub rel_path: String, pub size: u64, pub is_dir: bool }
#[derive(...serialize + specta...)]
pub enum DiffClass { OnlyInArchive, OnlyInFolder, SizeDiffers, Same }
pub struct ArchiveDiff { pub entries: Vec<(String, DiffClass)> }  // normalized rel path -> class

pub fn diff_archive(archive: &[archive::ArchiveEntry], folder: &[FolderEntry]) -> ArchiveDiff
```
Match file entries by **normalised relative path** (`\`→`/`, strip leading `./`), classify into the four
buckets by presence + size. Exclude directory entries from the file comparison (a dir is not a size-diff).
Deterministic ordering.

## Acceptance Criteria
- [ ] Added (`OnlyInArchive`), removed (`OnlyInFolder`), and `SizeDiffers` buckets computed correctly.
- [ ] Directory entries are excluded from file comparison (never reported as `SizeDiffers`).
- [ ] Slash normalisation makes `a\b.txt` and `a/b.txt` match; identical sets → all `Same`.
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default AND
      `--features index` builds; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-705 slice. Held in Backlog for the second
dispatch wave (independent of CPE-1054/1055/1056; kept back to avoid a merge-queue pile-up).
