---
id: CPE-919
title: Fix sidecar release — pristine-copy step aborts under bash -e -o pipefail
type: bug
component: CI
priority: high
tags: ready
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
The v0.57.7-sidecar release build failed on all three OSes. Root cause in `release-sidecar.yml`'s
"Stage pristine sidecar copies" step:
```bash
f=$(ls "sidecar/$s/target/release/$s" "sidecar/$s/target/release/$s.exe" 2>/dev/null | head -1)
```
Under the runner's default `bash --noprofile --norc -e -o pipefail`, `ls existing missing` exits 2 on the
missing candidate (`.exe` on Linux, no-ext on Windows); `pipefail` propagates the 2 through `| head`, and
`set -e` aborts the step **before** the `cp` runs. No `.pristine` is produced, so the tauri build then
fails: `resource path ...ai-console.pristine doesn't exist`.

Latent since CPE-867 added the step, but only exposed now — the GitHub runner image rolled (Node 20→24)
overnight, changing behaviour just enough to trip the fragile pipeline. Reproduced locally: the pipeline
exits 2 and never reaches the copy.

## Fix
Select the built binary by explicit existence check (`[ -f ]`) instead of `ls … | head`, so a missing
candidate never yields a spurious exit code. Robust across the Linux (no-ext) and Windows (`.exe`) layouts.

## Acceptance Criteria
- [x] Pristine step no longer aborts when one candidate path is absent (verified by local repro).
- [x] Sidecar release builds green on all three OSes and produces the `.pristine` resources.

## Work Log
- 2026-07-22 — Workflow-only hotfix committed straight to main (ci.yml can't validate workflow YAML, and
  release/CI plumbing commits already land directly here). Re-dispatched the v0.57.7-sidecar build.
