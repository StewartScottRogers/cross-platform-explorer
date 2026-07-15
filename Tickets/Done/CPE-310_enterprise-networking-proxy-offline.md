---
id: CPE-310
title: "Enterprise networking: proxy, offline & air-gapped"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [big-design]
estimate: 2-3h
created: 2026-07-13
closed: 2026-07-14
---

## Summary

"Logins to different envs" implies real-world/enterprise networks. Installs (package
managers), provider APIs, LM Studio, and catalog updates all hit the network and
must work behind corporate proxies, degrade gracefully offline, and be operable in
locked-down/air-gapped setups.

## Acceptance Criteria

- [x] Honour system/user proxy settings for all outbound calls (installs, provider
      verification, catalog fetch).
- [x] Offline: clear, actionable errors ([[CPE-299]]); cached catalog + already-
      installed agents keep working.
- [x] An air-gapped mode: install from local sources, disable remote catalog, no
      surprise outbound calls.
- [x] No secret leakage through proxy logs.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-282]], [[CPE-285]]. **Phase:** C3/C4. **Epic:** [[CPE-261]].

## Resolution

Closed by auditing every outbound surface to the code, confirming all four ACs are met by shipped +
tested mechanisms, and writing the consolidating operator doc that the "enterprise networking"
feature needed to be *operable*.

**What backs each AC (no new runtime code required — the pieces were built across CPE-347/369/376/308):**
- **Proxy for all outbound** — provider key check + catalog fetch both use `keyverify::resolve_proxy`
  (curl-convention `NO_PROXY`/`HTTPS_PROXY`/`ALL_PROXY`); installs inherit proxy env via the
  subprocess (no `env_clear`); LM Studio is loopback (correctly direct).
- **Offline** — `CPE_OFFLINE` short-circuits both remote paths with clear, non-blocking messages;
  last-known-good catalog + local installed agents keep working.
- **Air-gapped** — `CPE_OFFLINE` (env/policy switch) disables remote catalog + key-check; local
  installs via inherited package-manager config; no automatic call escapes the gate.
- **No secret in proxy logs** — HTTPS CONNECT tunnel for the key, no secret on the catalog path,
  `Redactor` on host logs.

**Files changed:** added `docs/enterprise-networking.md` (documentation only — no code change, so no
build needed beyond the existing green test suites cited above).

**Tradeoff / deferred:** a **GUI toggle** for `CPE_OFFLINE` is intentionally *not* built — the AC asks
for "an air-gapped mode", and an environment/policy switch is the standard enterprise mechanism
(IT-set, not end-user). A Settings toggle is noted as an optional future enhancement in the doc; file
a follow-up ticket if there is demand. This is the only piece from the CPE-369 "still open" list left
undone, and it exceeds the AC.

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-14 — First concrete slice landed as **CPE-369**: the `host.verify_key` egress (CPE-347)
now honours `HTTPS_PROXY`/`ALL_PROXY`/`NO_PROXY` and a `CPE_OFFLINE` switch (no surprise outbound
call; reported as an offline check, never blocking a save). Proxy/NO_PROXY/offline resolution is
pure + unit-tested; the key rides an HTTPS CONNECT tunnel so it isn't proxy-visible. **Still open
here:** proxy for installers/package-managers + catalog fetch + LM Studio; cached-catalog offline
behaviour; a user-facing air-gapped mode/UI. Remains `big-design` for those surfaces.

2026-07-14 — Picked up. Estimate: 2-3h (unchanged). Audited every remaining outbound surface
against the code and found the mechanisms already in place — the gap was that they were never
consolidated/verified as one feature. Findings:
  - **Catalog fetch** (`do_fetch_catalog` → `catalog_http_get`, CPE-376) already uses the same
    `resolve_proxy` and checks `CPE_OFFLINE` **first** (returns `offline:true`, no call). Manual and
    background auto-update both funnel through this one function, so none escape either gate.
  - **Installs** (`lifecycle::RealRunner`, `aggregate::Install`) spawn `npm`/`uv` via
    `std::process::Command` with **no `env_clear`** anywhere in the tree — the child inherits the
    app's full environment, so `HTTPS_PROXY`/`NO_PROXY`/registry config flow through natively. That
    is the correct, standard way package managers honour a proxy.
  - **LM Studio** (`lmstudio::RealProbe`) is a raw `TcpStream` to `127.0.0.1` — loopback, which must
    **never** be proxied. Direct connection is the correct behaviour; `NO_PROXY` also covers it.
  - **No secret via proxy logs**: key rides the HTTPS CONNECT tunnel (proxy sees host only), catalog
    carries no secret, installers get no injected secret, and `Redactor` scrubs host logs.
  - **Air-gapped**: `CPE_OFFLINE` is the single switch (env/policy — the enterprise norm) that
    disables both remote paths; local installs use inherited package-manager config.

2026-07-14 — Wrote the consolidating operator reference `docs/enterprise-networking.md` (the three
outbound paths, proxy config via curl-convention env vars, the `CPE_OFFLINE` air-gapped mode,
offline error behaviour, secret-safety, and where each is verified). Verified the pure
resolution logic green: `cargo test -p src-tauri --features sidecar-platform keyverify` — 9 passed
(`resolve_proxy_prefers_https_then_all_and_honours_no_proxy`, `no_proxy_matches_exact_suffix_and_wildcard`,
`offline_flag_parsing`). Catalog offline/last-known-good covered by the host catalog suite (91 passed).
