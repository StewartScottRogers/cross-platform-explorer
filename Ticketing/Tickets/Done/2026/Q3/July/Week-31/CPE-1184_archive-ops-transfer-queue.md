---
id: CPE-1184
title: "Route compress/extract through the streaming transfer queue with progress"
type: feature
component: Frontend
priority: low
status: Backlog
tags: ready
created: 2026-07-31
epic: CPE-705
---

## Summary
CPE-705 DoD gap flagged during decomposition. `doCompress`/`doExtract` are **blocking single `invoke`s**
(`App.svelte:2321,2339`), not routed through the transfer manager — so a large archive freezes the UI, against
the streaming-liveness convention ([[prefer-streaming-liveness]], docs/design/STREAMING.md). Route archive
compress/extract through `start_transfer` / the transfer queue with progress + cancel, like copy/move.

## Acceptance Criteria
- [ ] Compress + extract of a large archive show progress in the transfer queue and can be cancelled; UI stays
      responsive.
- [ ] Backend streams progress over an `ipc::Channel` per the STREAMING standard; `npm run check`/tests green.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-705) as the streaming follow-up; build after the core
  archive GUI tickets (1179–1183).
