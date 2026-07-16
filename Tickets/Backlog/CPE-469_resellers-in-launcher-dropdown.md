---
id: CPE-469
title: "Resellers in the launcher provider dropdown + reseller-key wiring to launch"
type: Feature
status: Open
priority: High
component: Sidecar (AI Console)
tags: [ready]
estimate: 1-2h
created: 2026-07-15
epic: CPE-467
---

## Summary
Surface every configured reseller in the launcher provider dropdown, and pass the reseller’s stored
key (CPE-452 reseller-key broker) into the launch. Uniform inline control (per prefer-inline-instant).

## Acceptance Criteria
- [ ] Provider dropdown lists native + lmstudio-local + every reseller descriptor.
- [ ] Selecting a reseller launches via its protocol template with the stored reseller key; a missing
      key surfaces the Keys panel (reseller tab), never a silent failure.
- [ ] The Model picker already filters by reseller (CPE-449/465) — selecting the reseller drives it.
- [ ] Launcher jsdom test: choosing a reseller sets the launch body + resolves the reseller key.
