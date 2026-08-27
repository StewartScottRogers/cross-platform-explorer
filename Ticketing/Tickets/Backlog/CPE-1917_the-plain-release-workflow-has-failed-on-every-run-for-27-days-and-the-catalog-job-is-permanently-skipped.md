---
id: CPE-1917
title: the plain Release workflow has failed on every run for 27 days — `verify-release-artifacts` cannot find `latest.json`, so the catalog job is permanently skipped
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-26
---

## Summary

Every run of the plain `Release` workflow (`.github/workflows/release.yml`) since **2026-08-04** has
failed, on **all three platforms**, at the step *"Verify updater manifest + signatures (CPE-1058)"*.
Because that step fails, the dependent `catalog` job is **skipped on every run** — it has not executed
once in 27 days.

This was noticed during the 2026-08-23 batched run, recorded as background context inside several
other tickets, and **never filed on its own**. Filing it now so it stops being folklore.

## Measured evidence

Run `32645968177` (`release: bump to 0.57.69`, 2026-08-23), job `release (windows-latest)`, raw log
fetched via `gh api repos/:owner/:repo/actions/jobs/97210161653/logs` (2,191 lines, complete — the
reporter view truncates, per CPE-1868, so this was fetched raw and the tail confirms a real finish):

    Running `target\release\verify-release-artifacts.exe --conf ../../src-tauri/tauri.conf.json --search ../../src-tauri/target`
    verify-release-artifacts: no latest.json found under ../../src-tauri/target (searched: 1)
    error: process didn't exit successfully: ... exit code 1
    ##[error]Process completed with exit code 1.

Job outcomes on that run:

| job | conclusion |
|-----|------------|
| `release (ubuntu-22.04)` | failure |
| `release (macos-latest, --target universal-apple-darwin)` | failure |
| `release (windows-latest)` | failure |
| `catalog` | **skipped** |

The three most recent `release.yml` runs (2026-08-20 and 2026-08-23 ×2) are all `failure`.

## What this does and does not affect

- It does **not** affect the **sidecar** build (`Release (sidecar-enabled)`), which is the channel
  users actually install ([[always-install-sidecar-build]]). That is why nobody has been hurt yet, and
  why it has gone unnoticed for a month.
- It **does** mean the plain channel's updater manifest has been unverified for 27 days, and the
  `catalog` job — the agent-catalog publish step (CPE-308) — has not run from this workflow at all.
- Related: **CPE-1874** (six shipped releases never signature-checked) and **CPE-1908** (the
  channel-purity guard only covers the plain manifest — the mirror-image gap).

## Acceptance criteria

- [ ] Establish *why* `latest.json` is absent under `src-tauri/target` in this workflow — is the
      verifier's `--search` root wrong for the plain build's layout, is the manifest emitted somewhere
      else (or under a different name) by the `tauri-action` version in use, or is the plain build
      genuinely not producing an updater manifest any more? Answer with evidence from a real run's
      artifact listing, not by reading the workflow.
- [ ] Fix the actual cause. If the search root is wrong, fix the root; if the manifest is genuinely
      not being produced, that is a bigger finding and must be stated plainly rather than papered over
      by widening the search.
- [ ] **Do not make the verify step non-fatal.** A verification step that can be skipped is the
      failure mode CPE-1058 was filed to close, and CPE-1901 is already open on a one-token kill
      switch in the neighbouring pin check. Silencing this is a regression, not a fix.
- [ ] Pin the fix with a test or guard that goes red if `latest.json` moves again — the recurring
      defect in this repo is guards that prove nothing.
- [ ] Confirm the `catalog` job actually runs to completion once the gate passes, and say what it
      published. A green `release` with a still-skipped `catalog` is a half fix.
- [ ] Add a backstop that makes a **month-long silent workflow failure** visible: nothing surfaced
      this for 27 days, which is the more expensive half of the bug.

## Notes

Filed 2026-08-26 by the sprint Foreman during pre-flight, from the previous run's checkpoint note
("open item 1") plus a fresh raw-log confirmation. The checkpoint explicitly flagged that this had
never been ticketed.
