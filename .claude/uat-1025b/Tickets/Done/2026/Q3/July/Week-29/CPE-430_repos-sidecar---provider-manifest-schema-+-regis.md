---
id: CPE-430
title: "Repos sidecar - provider manifest schema + registry"
type: Feature
status: Done
priority: High
component: Backend
tags: [ready]
estimate: 1-2h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-429
---

## Summary
Foundation of the forge epic (CPE-429): a provider-agnostic manifest schema + registry for the repos
sidecar, mirroring the AI Console agent registry (CPE-278). A provider is DATA - one manifest per
forge/VCS: id, name, VCS backend (git/hg/svn), auth model, capabilities, and the API host(s) it needs
(for allow-listed egress). Registry loads bundled + user manifests, skip-on-error.

## Acceptance Criteria
- [x] ProviderManifest (schema_version, id, name, kind, auth, api_hosts, capabilities, web_base).
- [x] ProviderRegistry::load_from_dirs - loads/validates *.json, skips malformed with a warning, user
      dir overrides bundled by id (like AgentRegistry).
- [x] Unknown-future schema_version refused; empty id / no kind refused.
- [x] Unit tests (load, override, skip-malformed, schema guard).

## Notes
Lives in a new sidecar/repos library crate (library-only first, like ai-console at CPE-277). Wire the
crate into the sidecar CI job.

## Work Log
2026-07-15 - Nightshift. Created the sidecar/repos library crate: ProviderManifest (schema_version/id/name/kind/auth/api_hosts/capabilities/web_base/experimental) + ProviderRegistry (load_from_dirs, skip-on-error, user-overrides-bundled, egress_allow_list union). Validation refuses empty id/name, unknown kind (git/hg/svn/perforce/fossil), and unknown-future schema. 4 unit tests, clippy clean. Wired the crate into the CI sidecar job. Provider manifests (data) are CPE-431.
