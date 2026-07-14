---
id: CPE-349
title: "Host: service the AI Console's capability requests (secrets.* dispatch loop)"
type: Feature
status: Done
closed: 2026-07-13
priority: High
component: Backend
created: 2026-07-13
---

## Summary

**Functional gate for CPE-287.** The sidecar can now *send* `secrets.*` requests (CPE-344) and
the host has all the parts — `supervisor::ProcessConnection::{recv,send}`, `broker::Broker`
(`register_provider`/`set_grants`/`dispatch`), and `providers::secrets::{SecretsProvider,
KeyringBackend}` — but `sidecar_start_ai_console` never runs a loop that reads the sidecar's
inbound `Request`s and answers them. So a key Save from the launcher UI (CPE-346) sends a
request the host never services → the sidecar's `BrokerClient` times out. The AI Console opens
and can launch agent sessions; only capability round-trips (secrets) are dead.

## Design
- After handshake, move the `ProcessConnection` into a servicing thread (don't just store it
  idle). Build a `Broker`, `register_provider(SecretsProvider::new(KeyringBackend))` (+ other
  granted providers), `set_grants("ai-console", consented)`.
- Loop: `conn.recv()` → on `Message::Request(req)`, `broker.dispatch("ai-console", &req)` →
  `conn.send(Envelope::new(id, Message::Response(resp)))` with the request's correlation id.
  Non-request frames (Status/Event) handled as today. Exit on EOF/stop.
- Reconcile with `AiConsoleState`: liveness/last-error/logs come from the thread; `stop`
  drops the connection to end it. Ensure only ONE reader owns `conn` (no split with diagnostics).

## Acceptance
- From the launcher Keys panel: Save a provider key → it lands in the OS keychain; the key list
  reflects it; Remove deletes it — no timeout. Denied capabilities still fail cleanly.
- Concurrency: no deadlock between the servicing thread, diagnostics, and stop.

## Notes
Discovered while demoing the `sidecar-platform` dev build for CPE-287. The CPE-344/345/346
layers are correct and unit-tested against a fake transport; this wires the real host end.

## Work Log
2026-07-13 (Dayshift) — Implemented in `src-tauri/src/lib.rs`.
- `AiConsoleState.conn` now holds a `ConsoleConn` control handle (stop flag + servicing-thread
  join handle) instead of the raw `ProcessConnection`. `Some` still means "running"; dropping
  it (stop/disable set the slot to `None`) sets the stop flag, so the thread exits and drops
  the real connection, reaping the child — the previous stop semantics are preserved (stop
  latency ≤ the 5s recv poll).
- `serve_ai_console_requests`: builds a `Broker`, registers the keychain `SecretsProvider`
  (`#[cfg(windows)]` — mac/linux backends land with CPE-322 and deny cleanly until then),
  `set_grants("ai-console", consented)`, then loops `recv → dispatch → send` answering the
  sidecar's `secrets.*` with the request's correlation id. Poll-timeouts re-check the stop
  flag; a closed connection ends the loop.
- `sidecar_start_ai_console` spawns that thread (moving the connection into it) and stores the
  handle.

Verified: builds under `--features sidecar-platform`; `cargo clippy --features sidecar-platform`
clean; `cargo test --features sidecar-platform` 61 pass. Ran live in `tauri dev` — the AI
Console launcher's **Keys** panel now reaches the Windows Credential Manager (Save/list/Remove)
instead of timing out. This is the functional completion of the CPE-287 credential workflow on
Windows.
