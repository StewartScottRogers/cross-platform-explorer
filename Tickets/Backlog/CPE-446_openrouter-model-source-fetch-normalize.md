---
id: CPE-446
title: "OpenRouter model source - fetch + normalize"
type: Feature
status: Open
priority: High
component: Backend
tags: [ready]
estimate: 2h
created: 2026-07-15
epic: CPE-444
---

## Summary
Turn OpenRouter's `GET /api/v1/models` response into `Vec<Model>`. The **parsing/normalization is pure** (unit-tested against a captured sample); the host performs the actual allow-listed GET (CPE-447).

## Acceptance Criteria
- [ ] `parse_openrouter_models(json) -> Vec<Model>`: maps id, name, context_length, pricing (prompt/completion), modalities; total on malformed input (empty list, no panic).
- [ ] Handles missing/extra fields gracefully; sorts stable (by id).
- [ ] Unit tests against a representative OpenRouter `/models` sample incl. malformed input.
- [ ] Documented mapping from OpenRouter fields to the common `Model` shape.

## Notes
Mirror `sidecar/repos/src/browse.rs::parse_github_contents`. Endpoint: https://openrouter.ai/api/v1/models .
