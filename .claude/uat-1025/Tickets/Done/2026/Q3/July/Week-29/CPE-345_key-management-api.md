---
id: CPE-345
title: "AI Console: provider key-management API (store/list/remove + format pre-check)"
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Backend
created: 2026-07-13
---

## Summary

Second CPE-287 sub-ticket. On the CPE-344 broker-secrets foundation, add the console HTTP API
to manage provider keys: store (keychain), list which providers have a key (names only, never
values), remove, and a **format pre-check**. Live network verification against the provider is
split to CPE-347 — ai-console has no TLS client and doesn't hold the `Network` capability, so a
real provider call needs that infra; the pre-check gives immediate "obviously wrong" feedback now.

## Scope
- `keycheck.rs`: pure `check_key_format(provider, key)` (non-empty; sane prefix for known
  providers like `sk-or-`/`sk-ant-`/`sk-`; lenient for unknown).
- `console.rs` routes: `POST /api/keys {provider,key}` (store), `GET /api/keys` → `{providers}`
  (probe known providers via `secrets.get`, values never returned), `POST /api/keys/delete
  {provider}`, `POST /api/keys/verify {provider,key}` → `{valid, live:false, detail}`.

## Acceptance
- Store/list/remove round-trip through the secrets store (tested with `MemSecrets`).
- List returns provider names only — never a key value.
- Format pre-check accepts plausible keys and rejects empty / clearly-wrong ones.
- `cargo test` + `cargo clippy` clean.

## Work Log
2026-07-13 (Dayshift) — Filed as the CPE-287 API layer.

2026-07-13 (Dayshift) — Implemented on branch `CPE-345-key-management-api`.
- `keycheck.rs`: pure `check_key_format` (non-empty/min-len/no-spaces; prefix rule for
  openrouter `sk-or-` / anthropic `sk-ant-` / openai `sk-`, matched by leading segment;
  lenient for unknown). 4 unit tests.
- `console.rs`: `POST /api/keys` (format-checked store), `GET /api/keys` (probe catalog
  providers, names only), `POST /api/keys/delete`, `POST /api/keys/verify` (offline pre-check,
  `live:false`). Round-trip test asserts store/list/delete AND that values never appear in the
  list response.
- `cargo test` 98 lib + 7 integration pass; `cargo clippy` clean.
- Live network verification split to **CPE-347** (needs the Network capability + host fetch
  provider; ai-console has no TLS client and shouldn't call out directly). Honest `live:false`
  in the response until then.
