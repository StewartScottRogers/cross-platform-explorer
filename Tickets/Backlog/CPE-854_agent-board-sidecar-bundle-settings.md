---
id: CPE-854
title: Agent Board sidecar — bundle in release + appears in Settings
type: feature
component: Multiple
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-21
epic: CPE-850
estimate: 3-4h
---

## Summary
Fourth child of CPE-850. Ship the sidecar: build + bundle `agent-board` (binary + `sidecar.json`) in
`release-sidecar.yml` alongside `ai-console`, so the installed host discovers its manifest and it appears
in **Settings → SidecarManager** — enable/disable, version, requested capabilities, and per-sidecar
diagnostics — exactly like AI Console / Repositories.

## Acceptance Criteria
- [ ] `release-sidecar.yml` builds `agent-board` and places its binary + manifest in the bundle overlay.
- [ ] After install, `agent-board` shows in the Settings sidecar manager with version + capabilities and
      can be enabled/disabled; diagnostics/health render.
- [ ] The plain (non-sidecar) build ships with no agent-board code (delete-test).
- [ ] GUI-verified alongside AI Console / Repositories; architecture note under `docs/design/`.

## Notes
Prereq: **CPE-851**, **CPE-853**. **GUI-verified — attended.**

## Work Log
