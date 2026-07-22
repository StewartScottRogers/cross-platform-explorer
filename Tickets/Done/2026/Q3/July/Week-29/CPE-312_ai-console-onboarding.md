---
id: CPE-312
title: AI Console first-run onboarding
type: Feature
status: Done
closed: 2026-07-14
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-13
---

## Summary

The console has many moving parts (agents, providers, models, keys, profiles). A
first-run flow that gets a user from zero to a working session — detect/install a
first agent, add a provider key securely, launch — turns a powerful-but-daunting
tool into an approachable one.

## Acceptance Criteria

- [ ] Guided first-run: pick + install an agent, add/verify a provider key
      ([[CPE-287]]), launch a session ([[CPE-289]]).
- [ ] Skippable; never blocks power users.
- [ ] Explains the security model in plain language (where keys live, what consent
      means).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-289]], [[CPE-287]]. **Phase:** C6. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.

## Work Log
2026-07-14 — Implemented on branch `CPE-312-onboarding`.
- `presets.rs`: `onboarded: bool` on `PresetStore` (persisted via host storage).
- `console.rs`: `POST /api/onboarded` sets it; the catalog already exposes it. Round-trip test
  (starts false → POST → true).
- `launcher.html`: a first-run **welcome overlay** shown once (when `presets.onboarded` is
  false) — the 3-step flow (pick/install an agent → optionally add a key via Keys… → set folder
  & Launch) and the security model in plain language (keys in the OS keychain; per-agent
  consent). "Get started" dismisses and persists.
- ai-console `cargo test` 107 lib + 7 integration, `clippy` clean.

Skippable ✔; explains where keys live and what consent means ✔. It's a welcome/guide overlay
rather than a forced wizard (least intrusive). Launcher visual needs an eyeball; the flag
round-trip is unit-tested.
