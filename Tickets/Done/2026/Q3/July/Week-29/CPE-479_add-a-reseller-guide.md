---
id: CPE-479
title: "Docs"
type: docs
status: Done
priority: Medium
component: Sidecar (AI Console)
tags: [1h]
estimate: Add-a-reseller extensibility guide
created: 2026-07-15
closed: 2026-07-16
epic: CPE-467
---

## Summary
Document how to add a new reseller purely as data — a unified manifest/descriptor — with no host or
per-agent code change. Mirrors the "add a new agent" guide (CPE-293).

## Acceptance Criteria
- [x] `docs/` guide: descriptor fields, the two protocol templates, egress allow-list entry, model
      endpoint, and how the picker + credentials pick it up.
- [x] A worked example (one reseller added end-to-end) a user can copy.
- [x] Linked from the AI Console docs + CPE-467 epic.

## Resolution
Wrote `docs/add-a-reseller.md`: the exact 5-step recipe (unified reseller manifest fields; the
host `models_egress` allow-list edit as the authoritative SSRF boundary; the KNOWN_RESELLERS +
bundled-descriptors + conformance wire-up; the agent `reseller_recipes[protocol]` side; the verify
commands) with a worked `acme` example and a 'when to skip' note (Perplexity/Portkey/Unify). Fulfils
the CPE-467 DoD 'adding a reseller is a manifest/descriptor edit only'. Nightshift loop 9.
