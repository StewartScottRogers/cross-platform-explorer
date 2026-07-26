---
id: CPE-1056
title: "Compress-selection planner — cpe_server::compress_plan (inner names + collisions)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-705
estimate: 3h
---

## Summary
Child of CPE-705 (Archive & compression suite). Add a **pure** planner that turns a user's selection of
files/folders into the inner archive-entry names a compress operation would use, detecting name collisions.
Backend-only, `cargo test` on the 3-OS matrix — no GUI, no user resource, **no new deps** (pure path logic).

## Design (buildable)
New module `crates/server/src/compress_plan.rs`, registered with `pub mod compress_plan;` in
`crates/server/src/lib.rs` **immediately after the line `pub mod checksum;`** (distinct anchor).

```rust
pub struct SelItem { pub path: String, pub is_dir: bool }   // a pre-walked selection entry
pub struct CompressPlan {
    pub entries: Vec<PlannedName>,     // source path -> inner archive name
    pub collisions: Vec<String>,       // inner names produced by >1 source
}
pub struct PlannedName { pub source: String, pub archive_name: String }

pub fn plan_compress(items: &[SelItem], flatten: bool) -> CompressPlan
```
Logic:
- Normalise separators (`\`→`/`).
- Default mode: strip the **common ancestor** of all sources so archive names are relative to it (e.g.
  `/a/b/c.txt` + `/a/b/d/e.txt` → `c.txt`, `d/e.txt`). Single item → its basename (or `name/...` for a dir).
- `flatten` mode: use each source's **basename** only (all files land at the archive root).
- Detect **collisions**: two distinct sources mapping to the same `archive_name` (common in flatten mode, or
  when same-named files live under different roots) → list the colliding inner name in `collisions`.
- Deterministic ordering.

## Acceptance Criteria
- [ ] Common-base stripping across nested paths yields the right relative inner names; single-file → basename.
- [ ] `flatten` mode maps every file to its basename and **surfaces the resulting collision** (e.g. two
      `README.md` from different dirs).
- [ ] Mixed-root selection handles the "no shared ancestor" case sanely (documented rule, tested).
- [ ] Forward/back-slash inputs normalise identically; empty selection → empty plan (no panic).
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default AND
      `--features index` builds; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a clean headless CPE-705 slice (the inner-naming
logic every compress command needs). Independent module; one-line lib.rs `pub mod` only.
