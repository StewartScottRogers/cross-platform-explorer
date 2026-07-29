---
id: CPE-768
title: Auto-updater endpoint 404s — latest stable release carries no latest.json
type: Bug
status: Done
priority: Medium
component: Updater
tags: ready
created: 2026-07-19
closed: 2026-07-19
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
- [x] The updater endpoint returns a valid `latest.json` (HTTP 200) for the current shipping channel.
      *(verified: endpoint now 200 — resolves to `v0.13.0`'s signed manifest after unshadowing; future
      sidecar releases serve the sidecar manifest.)*
- [x] An in-app update check succeeds (offers an update or reports up-to-date) instead of erroring.
      *(endpoint 200 + served version 0.13.0 < installed 0.52.0 ⇒ `check()` returns no update ⇒ "up to
      date", no error. Full "offers an update" fires once a newer sidecar release publishes on the fixed
      config — requires cutting a release, per the chosen direction.)*
- [x] The chosen approach is documented in RELEASING.md so future releases keep the endpoint valid.
      *(new "Auto-update channel" section.)*

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

## Work Log
2026-07-19 — Picked up. Estimate: 1-2h (kept). Plan: diagnose the endpoint resolution, then fix so an
update check no longer errors.

2026-07-19 — Investigation (this is a release-channel architecture gap, not a config typo):
- The updater endpoint is `releases/latest/download/latest.json`. `/releases/latest/` = newest
  **non-prerelease** release. That is currently **`v0.53.11-sidecar`** — a sidecar release that was
  (anomalously) published NOT-prerelease. Every other sidecar release is `prerelease: true`.
- Sidecar releases are **deliberately excluded from auto-update**: `release-sidecar.yml` sets
  `includeUpdaterJson: false`, `prerelease: true`, body says *"not part of the auto-update stream."* So
  `v0.53.11-sidecar` carries no `latest.json` → the endpoint 404s.
- Auto-update is designed around the **plain** channel (`release.yml`: `prerelease: false`,
  `includeUpdaterJson: true`). But the newest plain release is ancient (`v0.13.0`), and — critically — the
  **build users actually install is the sidecar build** (different app identity
  `…crossplatformexplorer.sidecar`), per [[always-install-sidecar-build]]. So the shipped app's updater is
  pointed at a channel that intentionally doesn't serve it.
- Frontend already degrades reasonably: the silent startup check swallows the error; a **manual** check
  (Application → Check for updates) shows "Couldn't check for updates right now…" — that is the "website
  down" the user saw. So the UX layer is fine; the real gap is that the sidecar build has **no valid
  update source**.
- Note `v0.53.14-sidecar` DID get a signed `latest.json`, but (a) it's a prerelease so `/latest/` skips it,
  and (b) its URLs point at the **plain** installer names (`Cross-Platform.Explorer_…`), not the
  `…Sidecar…` bundles — so even a resolving manifest would target the wrong app identity.

**This needs a product decision** (which channel serves auto-update for the sidecar build), and it touches
`release-sidecar.yml` — shared with the concurrent desktop release process. Options put to the user:
1. **Make the sidecar channel the auto-update stream** — `release-sidecar.yml` `prerelease:false` +
   `includeUpdaterJson:true`, ensure `latest.json` references the `…Sidecar…` installers + matching
   identity; endpoint then resolves. Real auto-update for what people run. Biggest change; shared workflow.
2. **Keep sidecar manual-only; stop it surfacing a broken updater** — feature-gate/hide the update check in
   sidecar builds (report "this build updates manually"), and re-mark `v0.53.11-sidecar` prerelease so the
   endpoint stops shadowing the plain channel. Coherent with current design.
3. **Band-aid now** — just re-mark `v0.53.11-sidecar` as prerelease so `/latest/` falls through to the old
   plain release's valid `latest.json` (HTTP 200, "up to date"), stopping the error. No code; not a real
   update path. Data-only `gh release edit`.
Awaiting direction before changing shared release infra.

2026-07-19 — Direction chosen (user): **make the sidecar channel the auto-update stream** (Option 1;
aligns with [[always-install-sidecar-build]]). Implemented:
- `src-tauri/tauri.sidecar.conf.json` — `bundle.createUpdaterArtifacts: false → true`, so sidecar builds
  generate a signed `latest.json` referencing the correctly-named `…Sidecar…` installers.
- `.github/workflows/release-sidecar.yml` — `prerelease: true → false`, `includeUpdaterJson: false → true`
  (kept `releaseDraft: true` for review); updated the release body/comments (this is the shipping
  auto-update channel, not a "not part of the auto-update stream" preview).
- `RELEASING.md` — new "Auto-update channel" section documenting that the published sidecar release is the
  update source, the non-prerelease requirement, and the version-bump gotcha.
- **Live endpoint fix:** re-marked the anomalous `v0.53.11-sidecar` as prerelease (it was non-prerelease
  but carried no `latest.json`, so it was shadowing the endpoint → 404). `/releases/latest/` now falls
  through to `v0.13.0`'s valid signed manifest. Verified: endpoint `HTTP 200`, served version `0.13.0`, and
  the GitHub API `/releases/latest` → `v0.13.0`. A 0.52.0 install therefore reports "up to date" with no
  error.

## Resolution
Root cause: the shipped **sidecar** build's updater (endpoint inherited from the base conf) pointed at
`/releases/latest/download/latest.json`, but the sidecar channel was intentionally excluded from
auto-update (`includeUpdaterJson:false`, `prerelease:true`, `createUpdaterArtifacts:false`) — and the one
non-prerelease sidecar release (`v0.53.11-sidecar`) carried no manifest, so every check 404'd.

Fix (per chosen direction — make sidecar the auto-update stream): enabled updater artifacts on the sidecar
overlay (`createUpdaterArtifacts:true`), and set the sidecar workflow to publish **non-prerelease** with
`includeUpdaterJson:true` so `/releases/latest/` resolves to the sidecar release and its signed manifest
references the `…Sidecar…` installers. Documented in RELEASING.md. Immediately unshadowed the live endpoint
by re-marking `v0.53.11-sidecar` prerelease → endpoint returns 200 (`v0.13.0`, "up to date") today; the
first sidecar release cut on the new config (via `/run` or the desktop train, with a version bump) becomes
the real update source. Files: `src-tauri/tauri.sidecar.conf.json`, `.github/workflows/release-sidecar.yml`,
`RELEASING.md`. Tradeoff: the config change can't be fully exercised until a sidecar release is published
on it (workflow_dispatch-only, not run by PR CI) — verified at the endpoint/manifest level.
