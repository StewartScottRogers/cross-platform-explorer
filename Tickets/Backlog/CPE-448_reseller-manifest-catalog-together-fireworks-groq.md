---
id: CPE-448
title: "Reseller manifest catalog (Together, Fireworks, Groq, ...)"
type: Feature
status: Open
priority: Medium
component: Backend
tags: [ready]
estimate: 2-3h
created: 2026-07-15
epic: CPE-444
---

## Summary
Declarative manifests for the researched resellers so each is onboarded as data (proves epic D3). OpenAI-compatible ones reuse one normalizer; GitHub Models / Eden get bespoke normalizers.

## Acceptance Criteria
- [ ] Manifests for: Together (`api.together.xyz/v1/models`), Fireworks (`api.fireworks.ai/inference/v1/models`), Groq (`api.groq.com/openai/v1/models`), DeepInfra, Novita, AI/ML API, WaveSpeed.
- [ ] GitHub Models manifest (`models.github.ai/catalog/models`) + its normalizer (publisher/modality/rate-limit shape, `X-GitHub-Api-Version`).
- [ ] A catalog test loads every manifest cleanly; known model-list hosts appear in egress_allow_list.
- [ ] Each manifest names its auth scheme + a normalizer id.

## Notes
Mirror `sidecar/repos/providers/*.json` + `tests/catalog.rs`. Endpoints captured in CPE-453 research.
