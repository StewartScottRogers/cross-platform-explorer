---
id: CPE-369
title: "Proxy- & offline-aware key-verification egress (slice of CPE-310)"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
estimate: 1h
created: 2026-07-14
closed: 2026-07-14
---

## Summary

The only outbound call the app fully owns today is the `host.verify_key` key check (CPE-347),
made with `ureq` — which ignores proxy environment variables by default. Deliver the concrete,
verifiable first slice of CPE-310 (enterprise networking): make that egress honour system proxy
settings and an offline switch. Bigger CPE-310 surfaces (installers, catalog fetch, LM Studio,
full air-gapped UI) stay in CPE-310.

## Acceptance Criteria

- [x] Key verification honours `HTTPS_PROXY` / `ALL_PROXY` (and lowercase), with `NO_PROXY`
      exclusions (exact host, domain suffix, `*`).
- [x] An offline switch (`CPE_OFFLINE`) skips the outbound call entirely — no surprise egress —
      and reports it as an offline (not a failed) check, never blocking a save.
- [x] Proxy/NO_PROXY/offline resolution is pure and unit-tested; the key never rides in a proxy-
      visible header (HTTPS CONNECT tunnels it).
- [x] `cargo test` (pure logic) + clippy clean; feature build compiles.

## Notes
Slice of [[CPE-310]]. HTTP/HTTPS CONNECT proxies only via `ureq::Proxy`; SOCKS would need ureq's
`socks-proxy` feature (out of scope here).

## Work Log
2026-07-14 — Filed as the implementable slice of CPE-310 while working the backlog.
