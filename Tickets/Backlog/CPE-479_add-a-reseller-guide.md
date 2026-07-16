---
id: CPE-479
title: "Docs"
type: docs
status: Open
priority: Medium
component: Sidecar (AI Console)
tags: [1h]
estimate: Add-a-reseller extensibility guide
created: 2026-07-15
epic: CPE-467
---

## Summary
Document how to add a new reseller purely as data — a unified manifest/descriptor — with no host or
per-agent code change. Mirrors the "add a new agent" guide (CPE-293).

## Acceptance Criteria
- [ ] `docs/` guide: descriptor fields, the two protocol templates, egress allow-list entry, model
      endpoint, and how the picker + credentials pick it up.
- [ ] A worked example (one reseller added end-to-end) a user can copy.
- [ ] Linked from the AI Console docs + CPE-467 epic.
