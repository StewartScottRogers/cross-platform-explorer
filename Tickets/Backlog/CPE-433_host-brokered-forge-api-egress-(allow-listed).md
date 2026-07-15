---
id: CPE-433
title: "Host-brokered forge API egress (allow-listed)"
type: Feature
status: Open
priority: High
component: Backend
tags: [ready]
estimate: 2h
created: 2026-07-15
epic: CPE-429
---

## Summary
The repos sidecar has no network client; the host performs allow-listed API calls on its behalf (no
SSRF), extending threat-model section 7. The allow-list is the union of each provider manifest api_hosts.

## Acceptance Criteria
- [~] host.forge_request {provider, method, path, body?}: host builds the URL from api_hosts (sidecar
      never supplies a full URL), attaches the stored token, returns the response; proxy/offline-aware.
      — **broker logic + handler done** (`forge_egress::forge_request` builds the URL host-side,
      injects the token, is proxy/offline-aware; `forge_request_response` returns `{ok,status,body}`).
      The `host.forge_request` **dispatch arm** lands with the repos sidecar's own host connection
      (CPE-432 AC3) — the handler is written ready to drop in, exactly like `verify_key_response`.
- [x] Only allow-listed hosts reachable; a path cannot escape the host; token never logged.
- [x] Unit tests on the URL builder + allow-list; threat-model row added.

## Resolution (core delivered + verified; dispatch arm gated on CPE-432 AC3)
Added `src-tauri/src/forge_egress.rs` (feature-gated like `keyverify`): the SSRF-safe host-brokered
egress core. `forge_api()` is the host-authoritative provider allow-list (github/gitlab/bitbucket/
codeberg/sourcehut/azure-devops fixed hosts; gitea/forgejo/GHE self-hosted). `build_forge_url()`
builds the URL host-side from `(provider, self_hosted?, path)` — **the sidecar supplies only a path**,
so a known provider can't be redirected. `validate_path()` blocks escape/injection (`//host`, `..`,
`://`, `@`, `\`, CR/LF, spaces). `is_blocked_ip()` classifies loopback/RFC1918/link-local/ULA/CGNAT/
metadata (`169.254.169.254`) incl. IPv4-mapped IPv6; `validate_self_hosted()` + call-time
`guarded_addrs()` re-check closes DNS-rebinding for user-entered self-hosted hosts. `forge_request()`
(behind `sidecar-platform`) makes the call reusing `keyverify`'s `resolve_proxy`/`is_offline`,
bounds the response body (8 MiB, DoS), and never logs the token. `forge_request_response()` in
lib.rs is the ready-to-wire handler. **7 unit tests** on the allow-list, method set, IP classifier,
path guard, self-hosted validation, URL builder, and address filter; `cargo test --features
sidecar-platform` green, `clippy --all-targets -D warnings` clean in **both** feature modes.
Threat-model row was added in CPE-440 (`forge-threat-model.md` §A). **AC2 + AC3 done; AC1 core done**,
its dispatch arm belongs to the CPE-432 repos host connection. Kept open until that wiring makes the
method callable end-to-end.

## Work Log
2026-07-15 - Picked up (user chose 'Build CPE-433 core now'). Estimate 2h. Plan: build the SSRF-critical PURE core host-side (src-tauri/src/forge_egress.rs, feature-gated like keyverify) — provider allow-list, host-side URL builder (sidecar never supplies a URL/host), path-escape guard, SSRF address classifier (loopback/private/link-local/ULA/metadata) + self-hosted host validation, auth-scheme injection. Live call behind sidecar-platform reusing keyverify resolve_proxy/is_offline. Full unit tests. The host.forge_request ROUTER arm needs the repos host connection (CPE-432 AC3) to exist, so it's written ready-to-wire but dispatch lands with that.
2026-07-15 - Landed forge_egress.rs (allow-list, build_forge_url, validate_path, is_blocked_ip/self-hosted guards, guarded_addrs, feature-gated forge_request) + forge_request_response handler. 7 unit tests pass under --features sidecar-platform; clippy --all-targets -D warnings clean in both modes. AC2/AC3 done; AC1 core done (dispatch gated on CPE-432 AC3). Keeping open.
