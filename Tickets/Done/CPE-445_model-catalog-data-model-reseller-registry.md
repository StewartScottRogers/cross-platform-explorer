---
id: CPE-445
title: "Model-catalog data model + reseller registry"
type: Feature
status: Done
priority: High
component: Backend
tags: [ready]
estimate: 2-3h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-444
---

## Summary
The provider-agnostic `Model` shape + a declarative reseller-manifest registry (like the forge providers CPE-430 / agent catalog CPE-278). Pure domain logic, unit-tested; no network. OpenRouter manifest first.

## Acceptance Criteria
- [x] `Model { id, reseller, display_name, context_length, pricing, modalities, capabilities, moderated }` with a total, defensive constructor.
- [x] `ResellerManifest { id, name, models_endpoint, auth, api_hosts, kind }` + a `ResellerRegistry` (load_from_dirs, get, all, egress_allow_list union) mirroring CPE-430.
- [x] Validation refuses empty id/name, unknown auth kind, missing endpoint; warnings surfaced not fatal.
- [x] Unit tests on the registry + validation; egress_allow_list is the union of every manifest's api_hosts.

## Notes
Mirror `sidecar/repos/src/providers.rs` (ProviderRegistry) exactly. This is the foundation the fetch/UI/snapshot build on.

## Resolution
Added `sidecar/ai-console/src/model_catalog.rs` (mirrors `agents.rs`/`repos/providers.rs`): the `Model` shape (id, reseller, display_name, context_length, `Pricing`, modalities, moderated), `ResellerManifest` + `ResellerRegistry` (load_from_dirs, get/all/len, warnings, `egress_allow_list()` = sorted-deduped union of api_hosts), with `validate()` refusing empty id/name, non-https endpoint, unknown auth/normalizer, or a future schema. First-class `resellers/openrouter.json` manifest. Pure + fully unit-tested (2 registry tests here + the parser tests under CPE-446). `cargo test` green, clippy clean. Complete — no runtime tail.
