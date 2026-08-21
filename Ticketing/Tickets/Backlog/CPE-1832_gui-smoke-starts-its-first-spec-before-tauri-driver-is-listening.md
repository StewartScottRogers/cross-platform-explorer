---
id: CPE-1832
title: gui-smoke starts its first spec before tauri-driver is listening, so a shard reds on a startup race
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

A `GUI smoke (ubuntu-latest) shard N` job can fail because WebdriverIO tries to open a session before
`tauri-driver` is accepting connections on `127.0.0.1:4444`. Observed 2026-08-21 on PR #972, run
`32434348500`, shard 2:

```
[0-0] Error serving connection: hyper::Error(User(Service), client error (Connect)
[0-0]     0: tcp connect error
[0-0]     1: Connection refused (os error 111))
[0-0] ERROR webdriver: WebDriverError: Request failed with error code UND_ERR_SOCKET
      when running "http://127.0.0.1:4444/session" with method "POST"
[0-0] FAILED in wry - file:///specs/archive-password.smoke.ts (1 retries)
```

Two things identify this as a startup race rather than a real defect:

1. **Only the first spec in the shard failed.** Every later spec in the same job (`[0-1]`, `[0-2]`, …)
   connected and ran normally — the driver was up by then. The one configured retry landed inside the
   same window and failed for the same reason.
2. **The PR could not have caused it.** #972 changed only `crates/s3` and `crates/vfs`, and `cpe-s3`
   is not a dependency of `cpe-vfs`, `cpe-server` or `src-tauri` — it is not compiled into the app
   binary the smoke test drives.

## Why it matters

The whole run's `— verdict across all shards` job fails with the shard, so a green PR is reported red.
The CI queue is already the throughput bottleneck on a busy sprint, and a full re-run costs another
complete matrix. It also erodes the signal: a leg that reds for infrastructure reasons trains people to
re-run rather than read, which is exactly how a real failure gets waved through.

## Acceptance criteria

- [ ] The harness waits for `tauri-driver` to be **accepting connections** before the first session is
      requested — poll the port with a bounded deadline, do not sleep a fixed duration. A fixed sleep
      trades a flake for a slower run and still races on a loaded runner.
- [ ] The wait has a deadline and fails with a message that says the driver never came up, so a genuine
      driver crash is still loud and is distinguishable from a slow start.
- [ ] The retry is not the mechanism relied on. A retry that re-enters the same startup window does not
      help; if a retry is kept, it must be able to outlast the wait.
- [ ] Verify by running the shard repeatedly under load — the failure is timing-dependent, so a single
      green run proves nothing. State how many runs were done and under what load.
- [ ] Check whether the other GUI-smoke entry points (the Windows nightly leg, any local runner script)
      share the same startup path and fix them together, or say explicitly why they do not.

## Notes

Filed by the Foreman after re-running run `32434348500` to unblock PR #972. Related infrastructure
tickets from the same sprint: CPE-1787 (package installs that could hang silently, fixed) and
CPE-1824 (the same hazard still live in the release workflows).

If this proves hard to close, an interim that is still worth having: make the failure message say
plainly that the driver was not listening, so the next person spends seconds rather than minutes
deciding whether it is real.
