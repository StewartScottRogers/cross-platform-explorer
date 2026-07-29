---
id: CPE-1130
title: QA — fold the cost-History visual residual into gui-smoke CI (burn down an MVD row)
type: Test
status: Open
priority: Medium
component: CI
estimate: 1h
created: 2026-07-29
closed:
tags: [ready]
---

## Summary

CPE-1114 (cross-session cost dashboard, in `Ticketing/Tickets/Done/2026/Q3/July/Week-30/`) shipped a
cost-**History** view that renders per-session bars (`.hd-bar` / `.hd-*` classes, in
`src/lib/components/AgentTimeline.svelte`). Its final visual residual is still **manually verified** —
an outstanding Manual Verification Debt (MVD) row the QA Architect wants automated away.

Extend the existing headless `gui-smoke` CI job (`.github/workflows/gui-smoke.yml`, driving the real
WebView2 build) to **seed a synthetic history fixture and assert the cost-History renders** on the real
build, so this surface is pinned by CI and never regresses to manual eyeballing.

## Acceptance Criteria

- [ ] The `gui-smoke` job seeds a synthetic history fixture (e.g. a small `history.jsonl`) the app reads.
- [ ] The smoke assertion navigates to the cost-History view and asserts `.hd-bar` / `.hd-*` elements
      actually render (non-empty), failing the job if they don't.
- [ ] The assertion is falsifiable — verify it FAILS when the fixture is empty / the selector is wrong,
      then passes with the real render (document the falsification in the PR).
- [ ] `gui-smoke` stays green end-to-end on the 3-OS/real-build path; no flakiness introduced.
- [ ] The corresponding row in `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` flips to ✅ naming this
      CI job as the pin, and MVD decrements.

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

*(Agent appends dated entries here throughout — do not fill in)*

## Notes

QA-Architect follow-up carried from the prior shift's CHECKPOINT.md. Read the CPE-1114 Done ticket +
`AgentTimeline.svelte` for the exact selectors/data shape. This is CI-infra: true verification is the
`gui-smoke` run going green with the new assertion (a worker self-verifies YAML + the fixture/selector
locally; the real UAT is the CI job). Prior gui-smoke research:
`.claude/research-library/entries/gui-smoke-devtoolsactiveport-webview2-ci.md` and
`headless-gui-smoke-test-tauri-driver.md`.
