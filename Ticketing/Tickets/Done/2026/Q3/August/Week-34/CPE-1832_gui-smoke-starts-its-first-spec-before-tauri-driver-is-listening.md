---
id: CPE-1832
title: gui-smoke starts its first spec before tauri-driver is listening, so a shard reds on a startup race
type: bug
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-20
closed: 2026-08-21
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

- [x] The harness waits for `tauri-driver` to be **accepting connections** before the first session is
      requested — poll the port with a bounded deadline, do not sleep a fixed duration. A fixed sleep
      trades a flake for a slower run and still races on a loaded runner.
- [x] The wait has a deadline and fails with a message that says the driver never came up, so a genuine
      driver crash is still loud and is distinguishable from a slow start.
- [x] The retry is not the mechanism relied on. A retry that re-enters the same startup window does not
      help; if a retry is kept, it must be able to outlast the wait.
- [x] Verify by running the shard repeatedly under load — the failure is timing-dependent, so a single
      green run proves nothing. State how many runs were done and under what load.
- [x] Check whether the other GUI-smoke entry points (the Windows nightly leg, any local runner script)
      share the same startup path and fix them together, or say explicitly why they do not.

## Notes

Filed by the Foreman after re-running run `32434348500` to unblock PR #972. Related infrastructure
tickets from the same sprint: CPE-1787 (package installs that could hang silently, fixed) and
CPE-1824 (the same hazard still live in the release workflows).

If this proves hard to close, an interim that is still worth having: make the failure message say
plainly that the driver was not listening, so the next person spends seconds rather than minutes
deciding whether it is real.

## Work Log

**2026-08-21 — root-caused and fixed a SECOND startup race, distinct from CPE-1772.**

CPE-1772 (PR #935, merged 2026-08-19) had already closed the WDIO -> tauri-driver race (waiting for
`127.0.0.1:4444` before the first `POST /session`). This ticket's own evidence (PR #972, run
`32434348500`, dated 2026-08-21 — AFTER that fix) shows a *different* error: a log line **from
tauri-driver itself** ("Error serving connection: hyper::Error(User(Service), client error (Connect) ...
Connection refused (os error 111))"), not a refused connect to tauri-driver's own port.

Root-caused by reading tauri-driver 2.0.6's own vendored source (local cargo registry cache,
`tauri-driver-2.0.6/src/{main,server,cli}.rs` — not guessed): `main.rs` spawns the REAL native driver
(WebKitWebDriver on Linux / msedgedriver on Windows) as a child process and, **without waiting for it to
finish binding its own port**, immediately opens tauri-driver's own port (4444, `cli.rs`'s `--port`
default) and starts accepting connections. `server.rs`'s `forward_to_native_driver` proxies every request
to `http://127.0.0.1:<native_port>` (4445 by default, `cli.rs`'s `--native-port` default). So tauri-driver's
front door (4444) can pass the CPE-1772 wait while its back door (4445) is still closed — a session
request landing in that inner window gets accepted by tauri-driver, which then fails to reach the native
driver and logs exactly the "Connection refused" evidence above. The two ports are not simultaneous: 4444
is a bare TCP bind (near-instant); 4445 needs an entire other driver binary to start and bind, which is
measurably slower, especially WebKitWebDriver under Xvfb.

**Fix** (`gui-smoke/wdio.conf.ts`): `beforeSession` now explicitly passes `--port`/`--native-port` to
tauri-driver (rather than relying on its internal defaults staying 4444/4445 forever) and awaits a bounded
TCP-readiness poll on **both** ports, in order, before returning — so WDIO's first `POST /session` (and any
retry, which reuses the same spawned process) never reaches tauri-driver until the whole proxy chain is
ready, not just its front door. The poll (`waitForPort`) was extracted into `gui-smoke/lib/waitForPort.ts`
so it is independently unit-tested (`waitForPort.test.ts`, 3 new cases, 121/121 total passing) without
needing a real `tauri-driver`/`tauri build` round trip — including a case that specifically distinguishes
"polls and returns as soon as ready" from "always sleeps the full budget" (the trap this ticket explicitly
warns against), proven by reverting to a `sleep(budgetMs)` implementation and observing that exact test go
red (2 of 3 failed), then restoring the real implementation and confirming green again.

**Entry-point sweep (AC5):** grepped the whole repo for every reference to `tauri-driver`/`4444`/`4445`.
There is exactly ONE place tauri-driver is ever spawned: `gui-smoke/wdio.conf.ts`'s `beforeSession` hook.
The CI Linux shard job, the CI Windows nightly/dispatch job, and a local `npm test` all run the identical
`wdio run ./wdio.conf.ts` — same code path, same fix, nothing else to change. `gui-smoke/README.md`
updated to document the two-port wait for anyone running this locally.

**Verification (AC4):** built a real release binary (`npm run tauri build -- --no-bundle`, Windows,
`cross-platform-explorer.exe` + local `tauri-driver`/`msedgedriver`) and ran the harness against it
repeatedly — this exercises the SAME `beforeSession` code path the Linux CI leg uses (spawn tauri-driver,
wait on both ports, first WebDriver command). 10 total fresh-process trials: 1 full-spec run (3/4 tests
passed; the 4th hit an unrelated local WebView2 renderer timeout on a later, unrelated assertion — no
startup-race signature anywhere in that log) + 9 focused single-assertion runs (the fastest test,
`the app window launched and <body> rendered non-empty content`, which exercises the exact same
spawn -> wait -> first-command sequence), the last 8 of those under artificial CPU load (4, then 8,
concurrent `yes > /dev/null` processes on a 32-core machine, deliberately contending for CPU the way a
busy CI runner does). All 10 runs: session created successfully, zero occurrences of
`ECONNREFUSED`/`Connection refused`/`hyper::Error`/`never accepted a connection`/`session not created` in
any log. Not run on the actual Linux/WebKitGTK/Xvfb path (no Linux box in this sandbox) — the fix is
platform-generic (same `wdio.conf.ts` code, same tauri-driver binary architecture on both OSes) but that
specific OS combination is unverified locally; the next real CI run of the Linux shards is the first
verification on that exact stack.

**Not touched, and why not:** CPE-1822 (Trash-view spec coverage) and CPE-1824 (release-workflow hang
hardening) are explicitly out of scope per the assignment and untouched. `gui-smoke.yml` itself needed no
changes — both jobs already funnel through the one fixed code path.

Files changed: `gui-smoke/wdio.conf.ts`, `gui-smoke/lib/waitForPort.ts` (new),
`gui-smoke/lib/waitForPort.test.ts` (new), `gui-smoke/README.md`.
