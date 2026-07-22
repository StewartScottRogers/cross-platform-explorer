---
id: CPE-888
title: Reject a leading-dash host in clone URLs (ssh ProxyCommand argument injection)
type: bug
component: Sidecar
priority: high
tags: ready
epic: CPE-259
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
`repos::clone::is_allowed_clone_url` — the hardened forge clone-URL allowlist (forge-threat-model §C) —
rejected a leading `-` on the **whole** URL but not on the **host** after a scheme or `user@`. So
`ssh://-oProxyCommand=evil/path` (and `ssh://git@-evil/path`, `git@-evil:owner/repo`) were **accepted**. Git
hands an ssh:// hostname to `ssh`, which parses a leading-dash host as an **option** — `-oProxyCommand=…`
gives **arbitrary command execution** (the CVE-2017-1000117 class). This module exists precisely to be
defense-in-depth against the "arbitrary command / off-transport" surface, so relying on git's own guard is a
gap.

Fix: after the scheme/`user@`, require the host to be non-empty and not begin with `-`, for `https://`,
`ssh://`, and scp-like URLs alike.

## Acceptance Criteria
- [x] `ssh://-oProxyCommand=evil/path`, `ssh://git@-evil/path`, `git@-evil:owner/repo`, `https://-evil/x`,
      and `ssh://git@/path` (empty host) are all refused.
- [x] Legitimate `https://…`, `ssh://git@host/…`, and scp-like `git@host:…` URLs still pass.
- [x] `repos` clone tests (6) + `cargo clippy --all-targets -D warnings` green.

## Work Log
- 2026-07-22 (autonomous) — Found the leading-dash-host gap while auditing the repos sidecar's clone
  hardening. Added a `host_ok` check (drop optional `user@`, reject empty / `-`-leading host) to every URL
  shape; added a regression test covering the ssh-injection payloads. 6/6 clone tests pass; clippy clean.
