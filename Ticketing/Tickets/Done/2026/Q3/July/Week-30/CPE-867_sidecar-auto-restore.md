---
id: CPE-867
title: Sidecar auto-restore from a pristine copy (L2 self-heal)
type: feature
component: Sidecar
priority: high
status: Done
tags: ready
epic: CPE-862
created: 2026-07-21
closed: 2026-07-21
---

## Summary
Epic CPE-862 L2 — true self-heal for a **missing or stale** sidecar binary, with zero user action. The
install can leave a stale sidecar when a running `--session-daemon` file-locks it and NSIS skips the
replacement (CPE-483); L1 (CPE-863) surfaced/repaired it, but couldn't *restore* a genuinely missing one.

## Approach
Bundle a **never-executed `.pristine` copy** of each sidecar (`sidecars/<id>.pristine`). Because nothing
ever runs a `.pristine`, a locked-file install skip can't leave *it* stale — so it's a trustworthy restore
source. On startup (after the orphan-daemon reap, so the exe isn't locked), for each sidecar: if the runtime
`sidecars/<id>.exe` is **missing or its bytes differ** from the pristine, rewrite it from the pristine.

Only applies to installed builds (a bundled resource dir); dev builds have no pristine and no-op.

## Acceptance Criteria
- [x] `restore_sidecar_from_pristine(exe, pristine)` restores when the exe is missing or differs; no-op when
      current or when no pristine exists. Unit-tested (no-pristine / missing / current / stale cases).
- [x] `restore_stale_sidecars_on_startup` runs in `setup()` after the daemon reap; logs what it restored.
- [x] Both sidecar bundle overlays ship a `.pristine` for each sidecar; the release workflow stages the
      `.pristine` copies after building the sidecars (a never-executed copy, so an install skip can't stale it).
- [x] `cargo test --features sidecar-platform` (restore test passes) + clippy `-D warnings` green in both
      feature modes. (End-to-end install self-heal = attended follow-up on the next installed build.)

## Work Log
- 2026-07-21 — Picked up autonomously after CPE-863 (L1). Implemented: pure `restore_sidecar_from_pristine`
  (unit-tested) + `restore_stale_sidecars_on_startup` sweep wired after the reap; `.pristine` staged in
  release-sidecar.yml and bundled in both overlays; the exe still ships directly (safe) with `.pristine` as
  the self-heal source. cargo test + clippy (both modes) green. Closing; L3 (resilient launch) is next.
