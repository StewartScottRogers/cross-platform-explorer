---
id: CPE-310
title: "Enterprise networking: proxy, offline & air-gapped"
type: Feature
status: Open
priority: Medium
component: Backend
tags: [big-design]
estimate: 2-3h
created: 2026-07-13
---

## Summary

"Logins to different envs" implies real-world/enterprise networks. Installs (package
managers), provider APIs, LM Studio, and catalog updates all hit the network and
must work behind corporate proxies, degrade gracefully offline, and be operable in
locked-down/air-gapped setups.

## Acceptance Criteria

- [ ] Honour system/user proxy settings for all outbound calls (installs, provider
      verification, catalog fetch).
- [ ] Offline: clear, actionable errors ([[CPE-299]]); cached catalog + already-
      installed agents keep working.
- [ ] An air-gapped mode: install from local sources, disable remote catalog, no
      surprise outbound calls.
- [ ] No secret leakage through proxy logs.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-282]], [[CPE-285]]. **Phase:** C3/C4. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-14 — First concrete slice landed as **CPE-369**: the `host.verify_key` egress (CPE-347)
now honours `HTTPS_PROXY`/`ALL_PROXY`/`NO_PROXY` and a `CPE_OFFLINE` switch (no surprise outbound
call; reported as an offline check, never blocking a save). Proxy/NO_PROXY/offline resolution is
pure + unit-tested; the key rides an HTTPS CONNECT tunnel so it isn't proxy-visible. **Still open
here:** proxy for installers/package-managers + catalog fetch + LM Studio; cached-catalog offline
behaviour; a user-facing air-gapped mode/UI. Remains `big-design` for those surfaces.
