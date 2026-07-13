---
id: CPE-322
title: "Cross-OS sidecar: macOS/Linux keychain backends, then extend the release channel"
type: Task
status: Open
priority: Medium
component: Backend
estimate: 4h
created: 2026-07-13
---

## Summary

The sidecar release channel (CPE-321) is Windows-only because the host's secrets
capability uses the OS keychain via `keyring`, which is currently gated to
`cfg(windows)` (`sidecar/host/Cargo.toml`). On macOS/Linux the provider falls back to an
in-memory backend, so secrets don't persist and aren't in an OS keychain — a violation of
the "secrets only in the OS keychain" invariant (ADR 0001 / CPE-268). We must not ship
sidecar installers on those platforms until their keychains are wired.

## Scope

1. Add `keyring` backends for macOS (Keychain) and Linux (Secret Service / keyutils) with
   the right feature flags per target in `sidecar/host/Cargo.toml`; make
   `providers::secrets` use the real backend on those OSes.
2. Verify a real round-trip on each (as was done for Windows Credential Manager).
3. Add per-OS bundle overlays (macOS/Linux binary is `ai-console`, no `.exe`) — e.g.
   `tauri.sidecar.unix.conf.json` mapping `.../release/ai-console` → `sidecars/ai-console`.
4. Extend `release-sidecar.yml` to a build matrix (add ubuntu + macos) selecting the right
   overlay per platform.

## Acceptance

- Secrets persist in the native store on macOS and Linux (verified round-trip).
- The sidecar release channel produces installers on all three OSes.

## Notes
Blocked-on nothing external; it's real backend work. Until done, CPE-321 stays Windows-only.
