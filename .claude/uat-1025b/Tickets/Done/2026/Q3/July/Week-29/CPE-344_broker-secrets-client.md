---
id: CPE-344
title: "AI Console: host secrets broker client + inject stored provider key at launch"
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Backend
created: 2026-07-13
---

## Summary

Foundation for CPE-287. The AI Console sidecar requests the `Secrets` capability and the host
has a real keyring-backed provider (`host::providers::secrets`, Windows Credential Manager),
but the sidecar has **no outbound request/response client** to actually call `secrets.*` over
its stdio envelope channel, and `vault::SecretAccess` has no production impl. Build that
client, a `SecretAccess` over it, wire response routing in `main.rs`, and use it: at launch,
inject the provider's stored key when the user didn't type one — so one key is shared across
every agent that uses that provider.

## Scope
- `broker_client.rs`: `BrokerClient` (allocate CorrelationId, write `Request` envelope, block
  on a per-id channel until `main` routes the matching `Response`; timeout). `BrokerSecrets`
  impl `vault::SecretAccess` (`secrets.set{name,value}` / `get{name}→{value}` / `delete`).
- `main.rs`: single shared writer; route inbound `Response` envelopes to the client.
- `console.rs`: `ConsoleState` holds an `Arc<dyn SecretAccess>`; `resolve_provider_key`
  injects the stored key at launch when none is supplied. Provider key namespace: `provider:<id>`.

## Acceptance
- Unit tests: request/response correlation (incl. timeout), `BrokerSecrets` round-trip over a
  fake transport, and launch key-resolution precedence (explicit > stored > none).
- `cargo test` + `cargo clippy` clean. Sidecar still builds and runs.

## Work Log
2026-07-13 (Dayshift, user chose to proceed with CPE-287) — Filing as the first CPE-287
sub-ticket; foundation the API (CPE-345) and UI (CPE-346) build on.

2026-07-13 (Dayshift) — Implemented on branch `CPE-344-broker-secrets-client`.
- `broker_client.rs`: `BrokerClient` (AtomicU64 ids, per-id mpsc waiter, shared-writer send,
  `recv_timeout`, `deliver` for the main loop to route `Response`s); `BrokerSecrets`
  (`SecretAccess` → `secrets.set{name,value}`/`get{name}→{value}`/`delete`); `MemSecrets`
  in-memory fallback. 5 unit tests (correlation, error surfacing, timeout+cleanup, set wire
  shape, mem round-trip) using a fake shared writer + id parsed off the wire (no fixed sleeps).
- `main.rs`: one `SharedWriter`; constructs the `BrokerClient`; routes inbound `Response`
  envelopes to it; hands `ConsoleState` a keychain-backed `BrokerSecrets` on Welcome.
- `console.rs`: `ConsoleState` holds `Arc<dyn SecretAccess>` (`new` defaults to `MemSecrets`,
  `with_secrets` for production); `provider_secret_name` + `resolve_provider_key` inject the
  provider's stored key at launch when none is typed (explicit > stored > none). +1 test.
- `cargo test` 93 lib + 7 integration pass; `cargo clippy --all-targets` clean.

Foundation done — next: CPE-345 (key-management API + verify endpoint), CPE-346 (launcher
credential UI), then close the CPE-287 umbrella.
