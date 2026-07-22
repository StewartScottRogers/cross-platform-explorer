---
id: CPE-347
title: "Live provider key verification (needs the Network capability)"
type: Feature
status: Done
priority: Low
component: Backend
created: 2026-07-13
closed: 2026-07-14
---

## Summary

CPE-345 added an *offline* key format pre-check (`/api/keys/verify` returns `live:false`).
True live verification — calling the provider (e.g. OpenRouter `GET /api/v1/key`, OpenAI
`GET /v1/models`) and reporting valid/invalid before saving — is deferred because ai-console
has no TLS HTTP client and does not hold the `Network` capability. The sidecar should NOT make
arbitrary outbound calls directly; route it through the host.

## Scope
- Request `Capability::Network` in `REQUESTED_CAPABILITIES`; host provides a `network.fetch`
  provider (allow-listed hosts) or equivalent.
- ai-console: a `verify_key(provider, key)` that issues `network.fetch` via the broker client
  (CPE-344) to the provider's check endpoint; map 2xx→valid, 401/403→invalid.
- `/api/keys/verify` returns `{valid, live:true, detail}`; the launcher UI shows the live result.

## Acceptance
- Entering a wrong key is rejected with a live "invalid key" before it's stored; a good key
  verifies. Allow-list prevents the capability being a general SSRF primitive.

## Work Log — 2026-07-14 (closed)

Implemented, but via a **host-controlled verify** rather than a general `network.fetch`/`Network`
capability — a narrower, safer design that satisfies the acceptance more directly:

- **No SSRF surface.** The sidecar sends `host.verify_key {provider, key}`; the *host* picks the
  URL from an allow-list, so the sandbox can never choose the target. `host.verify_key` can only
  ever hit three endpoints (OpenRouter `/api/v1/key`, OpenAI `/v1/models`, Anthropic `/v1/models`).
  This is why no `Capability::Network` / `network.fetch` provider was added — there is no general
  fetch primitive to allow-list.
- **`src-tauri/src/keyverify.rs`** — allow-listed endpoint map + pure `interpret_status` (2xx→valid,
  401/403→live rejection, everything else→inconclusive so a hiccup never blocks a save) + networked
  `verify_live` (behind `sidecar-platform`, using the new optional pure-Rust `ureq`/rustls dep).
- **`src-tauri/src/lib.rs`** — `serve_ai_console_requests` intercepts `host.verify_key` (mirroring
  `host.pick_folder`) → `verify_key_response`.
- **ai-console** — `HostDialogs` gained `verify_key` + a `KeyVerdict`; `handle_key_verify` now runs
  the offline shape check *first* (no network on an obviously-bad key), then the live check, and
  falls back to the offline result when no verifier exists / the call can't run.
- **launcher.html** — shows "Checking…", then ✓/✗ for a definitive provider answer, or the neutral
  offline result.

Tests: keyverify pure logic (endpoint allow-list + status interpretation); ai-console
`key_verify_passes_through_a_live_provider_verdict` and `key_verify_skips_the_live_check_for_a_bad_format_key`.
110 ai-console tests pass; clippy clean on both crates; feature build compiles.

**Verification note:** the actual outbound HTTPS round-trip (a real provider accept/reject) is
runtime-only — it can't be exercised headlessly, so the live yes/no wants a manual eyeball with a
real key. All pure logic, wiring, and the offline/fallback paths are unit-covered.
