---
id: CPE-023
title: release.ps1 aborts mid-release on git's stderr output
type: Bug
status: Done
priority: High
component: Packaging
estimate: 30m
created: 2026-07-11
closed: 2026-07-11
---

## Summary

`./scripts/release.ps1 -Version 0.2.0` bumped the version in all three manifests and then exited with
code 1 without committing, tagging, or pushing. It left the working tree in a half-released state:
versions bumped and staged, but no commit and no tag — so nothing built, and a naive re-run would have
produced a confusing double-bump.

Found while cutting v0.2.0 (i.e. by actually using the script, not by reading it).

## Environment

- OS: Windows 11, PowerShell 5.1
- git 2.55.0

## Steps to Reproduce

1. `./scripts/release.ps1 -Version 0.2.0`
2. Observe "Bumped version to 0.2.0…" then exit code 1, no commit/tag/push.
3. `git status` shows the three manifests staged but uncommitted.

## Expected Behavior

The script commits, tags `vX.Y.Z`, and pushes — or fails loudly *before* mutating anything.

## Actual Behavior

Version files mutated, then a silent abort at the first `git` call.

## Acceptance Criteria

- [x] Root cause identified
- [x] `release.ps1` completes commit + tag + push
- [x] Failures in git surface a clear message and a non-zero exit rather than a silent abort
- [x] Verified by actually cutting a release with it

## Resolution

**Root cause:** the script sets `$ErrorActionPreference = "Stop"`. Git writes ordinary progress and
status text to **stderr**, not stdout. Under `Stop`, PowerShell promotes any native-command stderr
output into a terminating `NativeCommandError` — so the very first `git add` aborted the script even
though git had succeeded (exit code 0). The version-bump code had already run, hence the half-released
state.

**Fix:** relax `$ErrorActionPreference` to `Continue` immediately before the git section, and add an
`Invoke-Git` helper that runs the command, echoes its output, and then checks `$LASTEXITCODE`
explicitly — failing loudly with the git exit code. This is the correct way to error-check native
commands in PowerShell: trust the exit code, not the stream the text came out on.

Verified by using the fixed script path to complete the v0.2.0 release (commit + tag + push), which
triggered a green build.

Files changed: `scripts/release.ps1`.

## Work Log

2026-07-11 — Hit this for real while cutting v0.2.0: versions bumped, exit code 1, no commit or tag.
2026-07-11 — Diagnosed: git writes normal output to stderr; $ErrorActionPreference="Stop" turns that into a terminating error despite git exiting 0.
2026-07-11 — Added Invoke-Git that checks $LASTEXITCODE instead of trusting the stream. Completed the v0.2.0 release. Closing as Done.

## Notes

Worth remembering for any other PowerShell script in this repo that shells out: with
`ErrorActionPreference = "Stop"`, a *successful* native command that happens to write to stderr will
still kill the script. Check `$LASTEXITCODE`.
