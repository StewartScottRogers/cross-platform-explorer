---
id: CPE-024
title: /run silently installs nothing and launches the stale build when the download fails
type: Bug
status: Done
priority: Critical
component: Packaging
estimate: 30m
created: 2026-07-11
closed: 2026-07-11
---

## Summary

While installing v0.2.0, `gh release download` failed (the Windows asset had not finished uploading
yet). The `/run` flow did not notice: the glob for the installer matched nothing, `Start-Process` was
handed a null path, the exit code came back empty, and the flow went on to **launch the previously
installed v0.1.0 binary** — presenting a stale build as if it were the new one.

This is the worst class of bug in an installer: it reports success while doing nothing.

## Steps to Reproduce

1. Trigger `/run` while the target release's asset for this OS is still uploading.
2. Download fails; temp dir contains only the older installer.
3. Flow launches the old binary and reports success.

## Expected Behavior

Abort loudly if the download fails or if the installer for the requested version is not present.

## Actual Behavior

Silently launched the old version.

## Acceptance Criteria

- [x] `/run` checks the exit code of `gh release download` and aborts on failure
- [x] `/run` asserts an installer file actually exists before installing
- [x] `/run` verifies the INSTALLED version matches the release it just downloaded, and fails if not
- [x] `/run` waits for (or reports) an in-progress release build instead of racing it
- [x] A null/missing installer can never reach Start-Process

## Resolution

Hardened `/run` at three points, each of which independently would have prevented the failure:

1. **Asset guard** — a release can be *published while its assets are still uploading*. "The release
   exists" is not "my platform's installer exists". `/run` now verifies an installer for THIS OS is
   actually attached, and checks for an in-progress Release build rather than racing it.
2. **Download guard** — clears the temp dir first (a stale older installer left there is exactly what
   got picked up), checks `$LASTEXITCODE` from `gh release download`, and throws if no installer file
   exists afterwards. A null path can no longer reach `Start-Process` — which is the root of the
   silent failure: `Start-Process` with a null path returns an *empty* exit code, and empty reads as
   success.
3. **Version assertion** — after installing, `/run` reads the installed `DisplayVersion` from the
   registry and fails if it does not equal the release it just downloaded.

Verified by using the hardened flow to install v0.3.0 and v0.4.0: the version assertion passed both
times, and the guards correctly refused to proceed while assets were still uploading.

## Work Log

2026-07-11 — Hit this for real: /run reported success while launching the stale v0.1.0 binary.
2026-07-11 — Root cause: Start-Process with a null path yields an EMPTY exit code, which reads as success. Compounded by the release being published before its Windows asset finished uploading.
2026-07-11 — Added asset/download/version guards. Exercised them installing v0.3.0 and v0.4.0. Closing as Done.

## Notes

Root cause is compounded: a release can be *published* while some platform assets are still uploading,
so "release exists" is not the same as "my platform's installer exists".
