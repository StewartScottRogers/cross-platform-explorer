---
id: CPE-1404
title: "Test: jsdom render-spec for DiskSpaceView (treemap cache + gen-token stream + refreshToken re-scan)"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-716
created: 2026-08-07
---

## Problem (hardening scout, Vein B — real streaming state machine)
`src/lib/components/DiskSpaceView.svelte` has real untested state machines: a per-path treemap cache, a
generation-token stream supersede (stale batch drop), and a `refreshToken` reactive re-scan-after-delete. Zero
coverage.

## Fix direction
Add `src/lib/components/DiskSpaceView.test.ts`. READ the component first for the real channel/rawInvoke API +
cache/gen-token/refreshToken mechanics (mirror the FileNameSearchDialog.test.ts approach for the supersede
case). Assert: a cache HIT for a previously-scanned path skips a re-scan; a stale batch from a SUPERSEDED scan
(gen-token) is dropped; a `refreshToken` bump busts the cache and forces a re-scan (the after-delete path).
Non-hollow. Report any real bug (don't fix). Test-only.
