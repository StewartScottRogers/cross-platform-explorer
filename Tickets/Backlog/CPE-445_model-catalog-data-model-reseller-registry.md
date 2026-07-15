---
id: CPE-445
title: "Model-catalog data model + reseller registry"
type: Feature
status: Open
priority: High
component: Backend
tags: [ready]
estimate: 2-3h
created: 2026-07-15
epic: CPE-444
---

## Summary
The provider-agnostic `Model` shape + a declarative reseller-manifest registry (like the forge providers CPE-430 / agent catalog CPE-278). Pure domain logic, unit-tested; no network. OpenRouter manifest first.

## Acceptance Criteria
- [ ] `Model { id, reseller, display_name, context_length, pricing, modalities, capabilities, moderated }` with a total, defensive constructor.
- [ ] `ResellerManifest { id, name, models_endpoint, auth, api_hosts, kind }` + a `ResellerRegistry` (load_from_dirs, get, all, egress_allow_list union) mirroring CPE-430.
- [ ] Validation refuses empty id/name, unknown auth kind, missing endpoint; warnings surfaced not fatal.
- [ ] Unit tests on the registry + validation; egress_allow_list is the union of every manifest's api_hosts.

## Notes
Mirror `sidecar/repos/src/providers.rs` (ProviderRegistry) exactly. This is the foundation the fetch/UI/snapshot build on.
