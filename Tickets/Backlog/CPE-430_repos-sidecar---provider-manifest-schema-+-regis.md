---
id: CPE-430
title: "Repos sidecar - provider manifest schema + registry"
type: Feature
status: Open
priority: High
component: Backend
tags: [ready]
estimate: 1-2h
created: 2026-07-15
epic: CPE-429
---

## Summary
Foundation of the forge epic (CPE-429): a provider-agnostic manifest schema + registry for the repos
sidecar, mirroring the AI Console agent registry (CPE-278). A provider is DATA - one manifest per
forge/VCS: id, name, VCS backend (git/hg/svn), auth model, capabilities, and the API host(s) it needs
(for allow-listed egress). Registry loads bundled + user manifests, skip-on-error.

## Acceptance Criteria
- [ ] ProviderManifest (schema_version, id, name, kind, auth, api_hosts, capabilities, web_base).
- [ ] ProviderRegistry::load_from_dirs - loads/validates *.json, skips malformed with a warning, user
      dir overrides bundled by id (like AgentRegistry).
- [ ] Unknown-future schema_version refused; empty id / no kind refused.
- [ ] Unit tests (load, override, skip-malformed, schema guard).

## Notes
Lives in a new sidecar/repos library crate (library-only first, like ai-console at CPE-277). Wire the
crate into the sidecar CI job.
