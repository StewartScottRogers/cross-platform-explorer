---
id: CPE-480
title: "Task"
type: ready
status: Open
priority: Medium
component: Sidecar (AI Console)
tags: [1-2h]
estimate: Reseller conformance tests + CI
created: 2026-07-15
epic: CPE-467
---

## Summary
A conformance kit that validates every reseller manifest: descriptor parses, both protocol templates
fill correctly, egress host is allow-listed, and the model-list shape normalizes. Runs in CI so a
malformed reseller cannot ship.

## Acceptance Criteria
- [ ] A data-driven test iterates every `resellers/*.json` and asserts descriptor + egress + recipe-fill.
- [ ] A reseller added purely as data passes with no code change (proves the extensibility claim).
- [ ] Wired into CI (both feature modes); clippy clean.
