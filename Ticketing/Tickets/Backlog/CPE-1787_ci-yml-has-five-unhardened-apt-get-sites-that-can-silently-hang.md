---
id: CPE-1787
title: ci.yml has five unhardened apt-get sites that can silently hang
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-19
closed:
---

## Problem

CPE-1772 (GUI-smoke shard hang) root-caused a class of silent, zero-output `apt-get` hang on
GitHub-hosted `ubuntu-latest` runners: `apt-get update` fetches every `InRelease` file, then stops —
no error, no further output — before fetching any `Packages`/`Translation` index. Reproduced live and
repeatedly on PR #935's own CI (`gui-smoke.yml`'s "Install Linux system dependencies", four separate
job instances across two different pushes, always at the exact same point). Current best account, not
conclusively proven: a GitHub-hosted-runner IPv6-connectivity stall (a runner advertises non-routing
IPv6, a client that tries it first hangs on the OS TCP timeout with zero output before ever falling
back to IPv4). Mitigated there with `Acquire::ForceIPv4=true` plus explicit `Acquire::Retries`/
`Acquire::http::Timeout`/`Acquire::https::Timeout`, and a `timeout-minutes` cap on every affected step
so a hang fails fast and named instead of riding to the job's full cap.

`ci.yml` has FIVE more `apt-get update && apt-get install` sites with none of that hardening — no
`ForceIPv4`, no explicit retries/timeouts, and (for four of the five) no step-level `timeout-minutes`
at all, so a hang here rides all the way to the job's default cap (no `timeout-minutes` is set on
either the `backend` or `crates` jobs, so GitHub's own 360-minute maximum is the only backstop):

1. `backend` job, "Install Linux system dependencies" (`ubuntu-latest` only) — installs the WebKitGTK
   dev packages the app compiles against.
2. `crates` job, "Install attr (getfattr) for xattr interop test" — required for the native-metadata
   OS-interop test to run for real instead of silently skipping.
3. `crates` job, "Install ffmpeg (video-thumb real-render test, Linux)" — `continue-on-error: true`,
   but that only helps once the step reaches a terminal state; with no `timeout-minutes` a genuine
   hang here never reaches one. **This is not hypothetical**: this exact step hung with zero output
   for 1.5+ hours on PR #935's own `Server crates (ubuntu-latest)` job while CPE-1772 was being
   investigated (observed live via `gh api .../jobs/<id>` step polling, `started_at` vs. wall clock),
   and had to be cancelled and rerun manually to get a verdict.
4. `sidecar` job, apt-get update + `libdbus-1-dev pkg-config` install.
5. `sidecar` job, `gnome-keyring` install (shares site 4's `apt-get update`, separate `install -y`).

`ci.yml` runs on every push and PR — far more often than `gui-smoke.yml` — so if the IPv6-stall
diagnosis is right, it applies here at least as often, and currently has no fail-fast behavior at all:
a hang is indistinguishable from the job simply being slow until someone notices it has been "running"
for an hour.

## What to do

- Apply the same hardening `gui-smoke.yml` now has to all five sites: `Acquire::ForceIPv4=true` plus
  explicit `Acquire::Retries`/`Acquire::http::Timeout`/`Acquire::https::Timeout` on both `apt-get
  update` and `apt-get install` invocations, and a `timeout-minutes` on each step sized against its
  normal cost (site 3 in particular — `continue-on-error: true` with no timeout is the exact
  configuration that let PR #935's live hang run for 1.5+ hours instead of failing fast and being
  swallowed by `continue-on-error` as designed).
- Confirm whether `ForceIPv4` actually reduces the STALL RATE here (not just whether the step still
  completes — a step can pass while still occasionally hanging on a bad roll) by watching a run of
  `ci.yml` for a hang before/after, same honesty standard CPE-1772 held itself to: a fix like this
  cannot be proven by one green run.
- Consider whether the `crates` job (and `backend`, which also has no job-level cap) should get a
  job-level `timeout-minutes` as a backstop, independent of the per-step hardening — GitHub's 360-minute
  default is not a meaningful cap for anything that matters in practice.

## Acceptance criteria

- [ ] All five `apt-get` sites in `ci.yml` get the same `ForceIPv4`/retry/timeout treatment as
      `gui-smoke.yml`'s two sites.
- [ ] Site 3 (`Install ffmpeg`) specifically gets a `timeout-minutes` so `continue-on-error` can
      actually do its job on a hang, not just on a fast failure.
- [ ] State honestly whether the fix measurably reduces hang frequency, or only makes an unavoidable
      hang fail faster/louder — do not claim more than the evidence supports.

## Notes

Filed by the CPE-1781/CPE-1772 PR (#935) reviewer during that PR's review — scoped out of that PR
(which is `gui-smoke.yml` + `wdio.conf.ts` only, per its own brief) so the diagnosis it produced isn't
stranded. See that PR's commits and comments for the full CPE-1772 root-cause writeup this ticket
extends to `ci.yml`.
