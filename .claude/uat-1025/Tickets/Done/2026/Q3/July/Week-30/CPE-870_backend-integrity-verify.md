---
id: CPE-870
title: Backend integrity verify — classify in Rust, ship only the report
type: feature
component: Sidecar
priority: low
status: Done
tags: ready
epic: CPE-737
created: 2026-07-21
closed: 2026-07-21
---

## Summary
On-demand integrity verify shipped the *entire* folder manifest to the webview to diff there (checksum_folder
+ verifyManifest). Move the diff into the backend: a `verify_folder(path, baseline)` command re-scans and
classifies in Rust, returning only the compact `IntegrityReport` (changed paths). Large trees stay
responsive, and the classifier is now available headlessly (foundation for a future scheduled verifier).

## Acceptance Criteria
- [x] `cpe_server::checksum::verify_manifest` mirrors the frontend `verifyManifest` (CPE-790) bitrot
      heuristic; `ChecksumEntry` gains `Deserialize`. Unit-tested (intact/edited/corrupted/missing/new).
- [x] Thin `verify_folder` command (spawn_blocking) registered; returns `IntegrityReport`.
- [x] `IntegrityDialog` verifies via the backend command (no full-manifest transfer); dead `verifyManifest`
      import removed.
- [x] cargo test + clippy (both modes) green; `npm run check` + integrity tests green.

## Work Log
- 2026-07-21 (autonomous) — Ported the classifier to Rust in cpe-server, added the command, adopted it in
  the dialog. Advances CPE-737 toward a headless/scheduled verifier.
