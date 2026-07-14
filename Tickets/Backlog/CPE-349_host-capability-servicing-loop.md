---
id: CPE-349
title: "Host: service the AI Console's capability requests (secrets.* dispatch loop)"
type: Feature
status: Open
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
