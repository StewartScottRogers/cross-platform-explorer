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
- 2026-07-22 (nightshift) — **Seam landed (foundation).** `invoke.ts` now routes every call through a
  pluggable `Transport` (interface + `localTransport` default + `setTransport`/`isRemoteTransport`); busy
  cursor + Diagnostics timing wrap the transport, so both survive the swap. Default is local Tauri IPC →
  in-process behaviour byte-for-byte unchanged. Unit tests: default→local passes args through; a swapped
  transport routes both `invoke` + `rawInvoke` and bypasses local IPC; `setTransport(null)` resets; a
  remote rejection still releases the busy cursor (invoke.test.ts 9/9). **AC #1 + #3 done.** Remaining
  (gated on CPE-820's reference server): a concrete remote `Transport` speaking the CPE-811 envelope, the
  3 streaming commands' over-the-wire form (AC #2), and GUI-verify against a loopback server (AC #4).
