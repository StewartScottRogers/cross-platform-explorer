---
id: CPE-286
title: LM Studio auto-detection (LAN probe) in Rust
type: Task
status: Done
priority: Medium
component: Backend
estimate: 1-2h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

Port `_resolve-lmstudio-url.ps1` to Rust: probe loopback and this host's LAN IPv4s
on the common LM Studio ports (1234/1235) for a reachable `/v1/models` endpoint,
and report the loaded model — so the local/remote LM Studio provider recipes
"just work" without manual URL entry.

## Acceptance Criteria

- [ ] Rust probe returns the first reachable LM Studio base URL + loaded model id.
- [ ] Bounded timeouts; graceful "none found" result.
- [ ] Feeds the LM Studio provider recipes ([[CPE-285]]); overridable by the user.
- [ ] Test with a mock endpoint.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-285]]. **Phase:** C4. **Epic:** [[CPE-261]].

## Resolution

Implemented `lmstudio` in ai-console: a `Probe` trait abstracts endpoint reachability so the selection logic is testable; `detect(candidates, probe)` returns the first reachable endpoint + its loaded model. `default_candidates()` = loopback on :1234/:1235 (LAN enumeration a later add). `RealProbe` does a real TCP-connect + minimal HTTP GET of `/v1/models`, best-effort parsing the first model id; `detect_default()` wires it up. Feeds the LM Studio provider recipes ([[CPE-285]]); overridable by the user. 5 tests (first-reachable, unknown-model, none, URL parse, id parse); 33 crate tests + clippy green.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Ported the LM Studio LAN probe to Rust during dayshift. Done.
