---
id: CPE-471
title: "Unified reseller manifest (launch + models + egress + auth); migrate the existing 9"
type: Feature
status: Open
priority: High
component: Sidecar (AI Console)
tags: [ready]
estimate: 2-3h
created: 2026-07-15
epic: CPE-467
---

## Summary
Collapse the split reseller data (model-list manifest + provider recipe + egress) into ONE reseller
manifest describing launch descriptor, model-list endpoint, egress hosts, and auth. Migrate the 9
existing `resellers/*.json` to the unified schema.

## Acceptance Criteria
- [ ] One `resellers/<id>.json` schema carries: id, name, protocol, base_url, auth, model endpoint,
      egress hosts, web link.
- [ ] The 9 existing resellers migrate with no behaviour change (model list still resolves).
- [ ] Loader + schema version; unknown fields ignored forward-compatibly.
- [ ] Tests for parse + one migrated reseller end-to-end.
