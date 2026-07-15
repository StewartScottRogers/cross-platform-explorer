---
id: CPE-446
title: "OpenRouter model source - fetch + normalize"
type: Feature
status: Done
priority: High
component: Backend
tags: [ready]
estimate: 2h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-444
---

## Summary
Turn OpenRouter's `GET /api/v1/models` response into `Vec<Model>`. The **parsing/normalization is pure** (unit-tested against a captured sample); the host performs the actual allow-listed GET (CPE-447).

## Acceptance Criteria
- [x] `parse_openrouter_models(json) -> Vec<Model>`: maps id, name, context_length, pricing (prompt/completion), modalities; total on malformed input (empty list, no panic).
- [x] Handles missing/extra fields gracefully; sorts stable (by id).
- [x] Unit tests against a representative OpenRouter `/models` sample incl. malformed input.
- [x] Documented mapping from OpenRouter fields to the common `Model` shape.

## Notes
Mirror `sidecar/repos/src/browse.rs::parse_github_contents`. Endpoint: https://openrouter.ai/api/v1/models .

## Resolution
`parse_openrouter_models(json)` in `model_catalog.rs`: parses OpenRouter's `/api/v1/models` `data[]`, mapping id, name, context_length, string-prices→f64 (prompt/completion), `architecture.input_modalities` (default `["text"]`), and `top_provider.is_moderated`. Total on malformed input (empty list, no panic; id-less entries skipped); sorted by id. Documented field mapping in the fn doc. 2 unit tests (rich sample incl. free model + missing fields; malformed/empty). `cargo test` green, clippy clean. Complete — the live GET is host-brokered egress (CPE-447).
