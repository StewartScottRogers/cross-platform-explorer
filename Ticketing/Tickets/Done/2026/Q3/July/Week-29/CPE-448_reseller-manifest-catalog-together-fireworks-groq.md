---
id: CPE-448
title: "Reseller manifest catalog (Together, Fireworks, Groq, ...)"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
estimate: 2-3h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-444
---

## Summary
Declarative manifests for the researched resellers so each is onboarded as data (proves epic D3). OpenAI-compatible ones reuse one normalizer; GitHub Models / Eden get bespoke normalizers.

## Acceptance Criteria
- [x] Manifests for: Together (`api.together.xyz/v1/models`), Fireworks (`api.fireworks.ai/inference/v1/models`), Groq (`api.groq.com/openai/v1/models`), DeepInfra, Novita, AI/ML API, WaveSpeed.
- [x] GitHub Models manifest (`models.github.ai/catalog/models`) + its normalizer (publisher/modality/rate-limit shape, `X-GitHub-Api-Version`).
- [x] A catalog test loads every manifest cleanly; known model-list hosts appear in egress_allow_list.
- [x] Each manifest names its auth scheme + a normalizer id.

## Notes
Mirror `sidecar/repos/providers/*.json` + `tests/catalog.rs`. Endpoints captured in CPE-453 research.

## Resolution
Added 8 reseller manifests under `sidecar/ai-console/resellers/` (together, fireworks, groq, deepinfra, novita, aimlapi, wavespeed, github-models) alongside openrouter — OpenAI-compatible ones use the `openai` normalizer; github-models uses the `github` normalizer + its catalog endpoint. New integration test `tests/resellers.rs` (3 tests): the full catalog loads with zero warnings, every manifest is coherent (https endpoint whose host ∈ its api_hosts), and known model hosts appear in `egress_allow_list()`. Endpoints for deepinfra/novita/aimlapi/wavespeed are flagged verify-before-live in CPE-453. `cargo test` green, clippy clean.
