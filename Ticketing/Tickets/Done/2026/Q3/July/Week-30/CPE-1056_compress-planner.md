---
id: CPE-1056
title: "Compress-selection planner — cpe_server::compress_plan (inner names + collisions)"
type: feature
component: Backend
priority: medium
status: Done
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
2026-07-25 (sprint) — Filed by the Product Manager as a clean headless CPE-705 slice (the inner-naming
logic every compress command needs). Independent module; one-line lib.rs `pub mod` only.

2026-07-25 (sprint, overnight Worker) — Built `crates/server/src/compress_plan.rs` +
`pub mod compress_plan;` after `pub mod checksum;` in `lib.rs`. Opened PR — see repo PR list for the
number. `cargo test -p cpe-server`: 775 passed, 0 failed (10 new). `cargo clippy --all-targets -- -D
warnings` and `--features index`: both clean. No new deps.

Assumptions logged (none of these are contradicted by the ticket, but they weren't spelled out, so
flagging for reviewer sign-off):
- **Single-item → basename falls out of one formula**, not a special case: the common-ancestor length is
  capped at `min(source segment count) - 1` so every item always keeps at least its own last segment.
  With one item that cap forces basename-only; with N items it also stops a directory item's full path
  from swallowing a shorter nested item's own name. Verified by test
  `deeper_nesting_still_strips_to_shared_ancestor` and `single_directory_also_maps_to_its_basename`.
- **Single lone directory → basename, no trailing slash** (the design note's parenthetical `name/...`
  wasn't required by acceptance criteria, so kept output uniform with the file case — a directory entry
  is just a name like any other; nothing downstream needs a `/` marker at the planner layer).
- **Mixed-root / no-shared-segment rule**: falls back to each source's full normalised path (leading
  separator stripped), e.g. `/a/x.txt`+`/b/y.txt` → `a/x.txt`, `b/y.txt`; documented in the module doc
  comment and covered by `mixed_root_selection_falls_back_to_full_relative_paths` (incl. a
  Windows-drive-letter variant, since drive letters differing is the most likely real-world mixed-root
  case).
- **Collision detection also fires outside `flatten`** when two sources resolve to the same relative
  name (e.g. the same path selected twice) — not just the flatten same-basename case the acceptance
  criteria called out by name. Covered by `non_flatten_collision_from_duplicate_selection`.
- Collision list order is first-repeat order over a `Vec` (not a `HashMap`) specifically to keep
  `plan_compress` deterministic per the acceptance criteria.

No blockers. Defender may flag the test binary as os error 225 on some machines — a scan artifact, not a
compile/test failure; `cargo test` above ran clean in this worktree.
