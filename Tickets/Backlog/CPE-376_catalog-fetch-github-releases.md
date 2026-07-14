---
id: CPE-376
title: "Host-mediated catalog fetch from GitHub Releases (CPE-308 part 2, slice 2c)"
type: Feature
status: Open
priority: Medium
component: Backend
tags: [ready]
estimate: 2-3h
created: 2026-07-14
---

## Summary

Download the signed catalog bundle from the app's GitHub Releases (the `releases/latest/download/…`
stable URL, right next to the installer) and apply it — then reload. Host-mediated so the sidecar
never touches the network directly.

## Acceptance Criteria

- [ ] `host.fetch_catalog` intercepted in `serve_ai_console_requests` (like `host.verify_key`):
      the **host** holds the source URL (default
      `https://github.com/StewartScottRogers/cross-platform-explorer/releases/latest/download/`,
      overridable via `CPE_CATALOG_URL`) — no general fetch exposed (no SSRF).
- [ ] Fetch `catalog-index.json` + `.sig`, then each listed `<id>.json` + `.sig`, via `ureq`
      (reuse `keyverify::resolve_proxy` + `CPE_OFFLINE`), into a staging dir.
- [ ] `sidecar_host::catalog::apply_bundle` with the embedded trusted pubkey; persist the version
      map; return `{ indexOk, applied, rejected }`.
- [ ] The sidecar calls `reload_catalog()` (CPE-375) on a non-empty apply.
- [ ] The app sets `CPE_AICONSOLE_CATALOG` (+ `CPE_AICONSOLE_CATALOG_KEYS` = embedded pubkey) when
      launching the sidecar, so bundled + fetched manifests both load.
- [ ] Unit-test the URL builder + proxy/offline decision + apply glue; note the live download is
      runtime-only-verifiable (like `host.verify_key`).

## Notes
Depends on [[CPE-375]], [[CPE-373]]. Trusted pubkey shipped by [[CPE-377]]. Part of [[CPE-308]].

## Work Log
2026-07-14 — Filed. Source = GitHub Release assets (user decision).
