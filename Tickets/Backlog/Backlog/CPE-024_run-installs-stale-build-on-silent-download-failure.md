---
id: CPE-024
title: /run silently installs nothing and launches the stale build when the download fails
type: Bug
status: Open
priority: Critical
component: Packaging
estimate: 30m
created: 2026-07-11
closed:
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

- [ ] `/run` checks the exit code of `gh release download` and aborts on failure
- [ ] `/run` asserts an installer file actually exists before installing
- [ ] `/run` verifies the INSTALLED version matches the release it just downloaded, and fails if not
- [ ] `/run` waits for (or reports) an in-progress release build instead of racing it
- [ ] A null/missing installer can never reach Start-Process

## Resolution
## Work Log
## Notes

Root cause is compounded: a release can be *published* while some platform assets are still uploading,
so "release exists" is not the same as "my platform's installer exists".
