---
id: CPE-1127
title: "Checkpoint store: validate manifest_id is a single safe segment (read-path hardening)"
type: chore
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-27
epic: CPE-732
---

## Summary
Non-blocking finding from the CPE-1123 (#439) opus review. `checkpoint_store` passes `manifest_id` unsanitized to
`load_manifest` → `manifest_path(store_dir, id)` = `store_dir/manifests/{id}.json`. A crafted `manifest_id`
containing `..` could make the **read** resolve to an arbitrary `.json` outside the manifests dir. Impact is
bounded (read-only; must end `.json` and parse as `PersistedManifest`; `manifest_id` normally comes straight from
`checkpoint_list`) and the **write/delete side is already independently guarded** by `revert_engine::safe_segments`
— so it cannot corrupt or delete files. Defense-in-depth on a security-adjacent (checkpoint/rollback) feature.

## Design (small)
- Add a `validate_manifest_id(id) -> Result<>` helper that rejects any id that isn't a single safe segment (no
  `/`, `\`, `..`, `:`, no leading/trailing dots) and call it at the store entry points before `load_manifest`
  (`checkpoint_store` preview/revert/revert_one — lines ~246/273/290).
- **Also cover the pre-existing sites** the reviewer flagged with the same latent pattern: `snapshot_capture::restore`
  and `prune` (they take a manifest id / path the same way). One shared validator across all callers.

## Acceptance Criteria
- [ ] A `manifest_id` containing `..`/separators/`:` is rejected with a clean error at every store entry point
      (checkpoint_store preview/revert/revert_one + snapshot_capture restore/prune) before any manifest read.
- [ ] A unit test feeds a traversal id and asserts it's refused (no read outside `manifests/`); valid ids still
      work. `cargo test` + `cargo clippy -D warnings` (both modes) green; no new deps.

## Work Log
2026-07-27 (sprint) — Filed from the CPE-1123 #439 review (low-severity, write-safe read-path traversal;
pre-existing pattern in restore/prune too). Merged #439 as-is (bounded + write-safe) and captured hardening here.

2026-07-27 (sprint) — Built (PR #440, merged 324ce05a). Reviewer APPROVE + UAT PASS: validate_manifest_id at the single load_manifest chokepoint (grep-verified to cover restore/prune/manifest_snapshot -> all 5 entry points); rejects ../ separators/:/NUL unconditionally (both OS separators). UAT regression-probe removed the guard -> planted-outside-file test FAILED, proving it load-bearing; valid ids unaffected; clippy clean all modes.
