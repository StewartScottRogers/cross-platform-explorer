---
id: CPE-863
title: Sidecar health diagnosis + Repair button (L1) + bundle repos
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
First pass of sidecar self-healing (epic CPE-862, L1): make it obvious *why* a sidecar isn't working and
give a one-click Repair, instead of the current vague "isn't available" dead-end. No auto-restore yet
(that's L2) — this is diagnosis + the repairs we can do without a pristine copy.

## Deliverables
1. **Bundle `repos`** — add `repos` (binary + `sidecar.json`) to `tauri.sidecar.windows.conf.json` and
   `tauri.sidecar.unix.conf.json`; it was never bundled, so it's a genuinely missing sidecar.
2. **Binary health signal** — a generic `resolve_sidecar_bin(app, id)` and a `binary_ok: bool` on
   `SidecarInfo`, so the UI knows whether each sidecar's binary actually resolves.
3. **Status + reason in Settings → Platform** — `SidecarManager` shows a per-sidecar status pill
   (Missing / Disabled / Incompatible / Error / Running / Ready) derived from binary_ok + enabled +
   compatible + running + last_error, with the plain-language reason.
4. **Repair action** — `sidecar_repair(id)`: reap orphan session-daemons, drop a wedged connection, and
   clear the stored last-error so a stuck sidecar can start clean; re-checks and returns. A **Repair**
   button per row. (A truly missing binary is reported honestly — restore is L2.)
5. **Better launch-failure message** — when a start returns null, point the user to Settings → Platform
   ("see why + Repair") instead of "isn't available in this build".

## Acceptance Criteria
- [x] `repos` is bundled in both sidecar configs.
- [x] `sidecar_details` returns `binary_ok`; `SidecarManager` shows the status + reason per sidecar.
- [x] `sidecar_repair` reaps daemons + clears error/connection; a Repair button invokes it and refreshes.
- [x] Launch-failure notices direct to Settings → Platform.
- [x] `cargo test` + clippy (both feature modes) green; `npm run check` + suite green; GUI-verified.

## Work Log
- 2026-07-21 — Filed under epic CPE-862 after the sidecar flakiness this session. User chose L1
  (diagnose + Repair) first. Branch `cpe-863-sidecar-health-repair` off main.
- 2026-07-21 — Implemented: bundled repos (+ its release build step); generic `resolve_sidecar_bin` +
  `binary_ok`; per-sidecar status pill + Repair button in `SidecarManager`; `sidecar_repair` command;
  clearer launch-failure notice; `mgr.st*`/`mgr.repair*` across all 12 locales. check clean; 902 tests;
  clippy green both feature modes. Merged via PR #138. Closing.
