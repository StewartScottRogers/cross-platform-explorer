---
id: CPE-1048
title: "Fix gui-smoke CI: WebView2 'session not created / DevToolsActivePort' on windows-latest"
type: bug
component: CI
priority: high
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-616
estimate: 3h
---

## Summary
The CPE-1045 `gui-smoke` CI job **fails on GitHub `windows-latest`** at WebDriver session creation:
`WebDriverError: session not created: DevToolsActivePort file doesn't exist` (after a 60s wait). The same
harness runs **green on a real desktop** (worker + reviewer both ran it live) — so it's a CI-environment
issue: WebView2 won't launch/attach on the stock GitHub runner. Until this is green the job doesn't pin
burndown #1/#2 (it just reds every push). The core release pipeline (`ci.yml`, `release-sidecar.yml`) is
unaffected.

## Investigate + fix (see the Researcher's findings / Library entry)
Likely angles: WebView2 Runtime / Edge / `msedgedriver` version alignment on the runner; required WebView2
launch args (user-data-dir, `--disable-gpu`, no-sandbox); tauri-driver ↔ msedgedriver handshake; whether a
real desktop session / a short pre-warm is needed; timeout. If a stock `windows-latest` genuinely can't run
it, the honest fallback is to (a) make the job `workflow_dispatch`-only or `continue-on-error` (non-blocking)
with a clear note, and/or (b) note a self-hosted/attended path — rather than leave main permanently red.

## Acceptance Criteria
- [ ] The `gui-smoke` job is **green on `main`** (WebView2 session creates, the 3 smoke assertions pass) —
      OR, if not achievable on stock runners, the workflow is made non-blocking (`workflow_dispatch` /
      `continue-on-error`) with the reason documented, so main isn't perpetually red.
- [ ] The harness launches the app in `--test-mode --x=-4000` (off-screen + non-focused) per CPE-1046/1047
      so even in CI it can't grab a display (and matches the anti-disruption convention; note the `=` form
      for negative geometry).
- [ ] Burndown #1/#2 flip to ✅ (with the pinning job named) only once the job is actually green; else they
      stay 🔧 with a note.

## Work Log
2026-07-25 — Filed after the gui-smoke job failed on main (DevToolsActivePort). Core pipeline unaffected.
