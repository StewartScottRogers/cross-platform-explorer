---
id: CPE-389
title: "AI Console UX: clarify 'Save preset' vs keys; + /output endpoint test"
type: Task
status: Done
priority: Medium
component: Frontend
tags: [ready]
created: 2026-07-14
closed: 2026-07-14
---

## Summary

Gap-analysis follow-ups (items 2+3). The "Save set" control read like it saved the API key (it saves
the agent's provider+model config); renamed to **"Save preset"** with an explicit tooltip + success
message pointing keys to Keys…, dropdown/label "Preset". Also added the missing `/api/session/{id}/
output` endpoint test (unknown session → 400).

## Acceptance Criteria
- [x] "Save set" → "Save preset"; placeholder/label/dropdown/messages say preset + "not your key".
- [x] `/api/session/{id}/output` unknown-session test.
- [x] 331 vitest, 126 ai-console tests pass; clippy clean; launcher JS syntax OK.

## Work Log
2026-07-14 — Done alongside the launcher harness (CPE-388).
