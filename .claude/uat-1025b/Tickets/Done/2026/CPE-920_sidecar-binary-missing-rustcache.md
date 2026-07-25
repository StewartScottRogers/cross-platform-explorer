---
id: CPE-920
title: Sidecar release — Windows binary missing after cargo build (rust-cache prune)
type: bug
component: CI
priority: high
tags: ready
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
After CPE-919 fixed the pristine-copy pipefail abort, the v0.57.7-sidecar build went green on Linux and
macOS but Windows still failed with `resource path ...ai-console.pristine doesn't exist`.

Root cause is separate: `swatinem/rust-cache` prunes each cached workspace's **own** final binaries before
saving the cache (to keep it small). On a cache hit the deps restore but `ai-console.exe` is absent, and
`cargo build --release` reports "Finished" off the fresh fingerprint without relinking the pruned binary.
The pristine step's `[ -f ]` check then correctly finds nothing and silently continues, so `build.rs` later
fails on the missing bundled resource. It bit Windows this run because the ai-console crate changed
(CPE-913/914/915) but the cache key (Cargo.lock + rustc, not source) still hit.

## Fix
Make the pristine step self-healing and self-diagnosing: if a sidecar binary is missing, `touch` its
sources (to force-invalidate the fingerprint) and rebuild; if it's *still* missing, fail loudly with a
`release/` listing instead of shipping a bundle with a missing resource.

## Acceptance Criteria
- [x] Missing sidecar binary is force-rebuilt before the pristine copy.
- [x] Hard failure (with a directory listing) if a binary truly can't be produced — no silent broken bundle.
- [x] v0.57.7-sidecar builds green on all three OSes with a Windows installer asset.

## Work Log
- 2026-07-22 — Follow-up to CPE-919 (same step, same release incident). Workflow-only; committed to main
  and re-dispatched the build.
