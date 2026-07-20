---
id: CPE-768
title: Auto-updater endpoint 404s — latest stable release carries no latest.json
type: Bug
status: Open
priority: Medium
component: Updater
tags: ready
created: 2026-07-19
closed:
estimate: 1-2h
---

## Summary
The Tauri auto-updater reports the update endpoint as unreachable ("the website is down"). The configured
endpoint in `src-tauri/tauri.conf.json` is:

```
https://github.com/StewartScottRogers/cross-platform-explorer/releases/latest/download/latest.json
```

Fetching it returns **HTTP 404**. `/releases/latest/` resolves to the latest **non-prerelease, non-draft**
release, currently `v0.53.11-sidecar`, which has **no `latest.json` asset** — the sidecar release channel
sets `createUpdaterArtifacts: false` (see `src-tauri/tauri.sidecar*.conf.json`), so those builds never emit
`latest.json`. Net effect: every update check 404s.

## Environment
- Discovered: 2026-07-19 while GUI-verifying CPE-766 in a dev build (updater surfaced the error).
- Endpoint: `releases/latest/download/latest.json` → 404 (redirects to `releases/download/v0.53.11-sidecar/latest.json`).

## Steps to Reproduce
1. `curl -sSL -o /dev/null -w '%{http_code}' https://github.com/StewartScottRogers/cross-platform-explorer/releases/latest/download/latest.json` → `404`.
2. Or: run the app and trigger an update check → endpoint error.

## Expected Behavior
The update check resolves a valid `latest.json` describing the newest installable build, so the updater can
compare versions and offer an update (or cleanly report "up to date").

## Actual Behavior
404 → updater treats the endpoint/"website" as down. No update check ever succeeds.

## Root Cause
Channel/endpoint mismatch:
- The updater endpoint expects `latest.json` on the release that `/releases/latest/` points to.
- `/releases/latest/` = latest **non-prerelease** release. The shipping channel is **sidecar prereleases**
  (`isPrerelease: true`), which `/releases/latest/` skips, and those builds also don't emit `latest.json`
  (`createUpdaterArtifacts: false`).
- So the "latest stable" release (`v0.53.11-sidecar`) has no `latest.json`, and the newer one that *does*
  (`v0.53.14-sidecar`) is a prerelease and is skipped.

## Acceptance Criteria
- [ ] The updater endpoint returns a valid `latest.json` (HTTP 200) for the current shipping channel.
- [ ] An in-app update check succeeds (offers an update or reports up-to-date) instead of erroring.
- [ ] The chosen approach is documented in RELEASING.md so future releases keep the endpoint valid.

## Notes — candidate fixes (pick at pickup)
1. **Emit `latest.json` on the sidecar channel** (`createUpdaterArtifacts: true` for the sidecar builds) and
   point the endpoint at the sidecar release that carries it — but `/releases/latest/` ignores prereleases,
   so either stop marking sidecar releases as prereleases, or use a fixed/tagged endpoint instead of
   `/latest/`.
2. **Point the endpoint at a stable channel** that always has `latest.json` (e.g. the plain `release.yml`
   output, which sets `createUpdaterArtifacts: true`), and ensure that channel is actually published.
3. **Pin the endpoint to a moving "release" tag** whose `latest.json` is refreshed on each publish.
Cross-cutting with the sidecar-vs-plain release strategy and [[always-install-sidecar-build]] /
[[sidecar-platform-program]]. Pre-existing; surfaced during CPE-766 verification, not caused by it.
