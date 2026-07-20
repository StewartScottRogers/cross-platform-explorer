---
id: CPE-819
title: Frontend pluggable transport seam (local IPC vs remote RPC) + streaming
type: feature
component: Frontend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-20
epic: CPE-810
estimate: 3-4h
---

## Summary
Child of CPE-810. Make `src/lib/invoke.ts` — the single call chokepoint — route to either **local
Tauri IPC** or a **remote RPC** transport speaking the CPE-811 envelope, chosen by config, with the
GUI above it unchanged. Provide the network-streaming equivalent for the 3 `ipc::Channel` producers
(dir listing, name search) so the streaming-liveness convention survives the swap. Preserve the
busy-cursor wiring. Prereqs: CPE-811, CPE-815.

## Acceptance Criteria
- [ ] `invoke.ts` selects local-IPC or remote-RPC transport; call sites unchanged.
- [ ] The 3 streaming commands have a working over-the-wire form (batched, first rows paint immediately).
- [ ] Busy cursor + Diagnostics timing intact across both transports.
- [ ] GUI-verified against a local server through the remote path (loopback).

## Work Log
