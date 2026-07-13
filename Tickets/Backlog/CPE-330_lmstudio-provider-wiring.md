---
id: CPE-330
title: "Wire LM Studio local provider (URL auto-detect) into manifests"
type: Feature
status: Open
priority: Medium
component: Backend
created: 2026-07-13
---

## Summary

The reference has a first-class `local-lmstudio` provider per agent: auto-detect the LM
Studio URL (probe loopback + LAN IPv4s on :1234/:1235), `lms load <model>`, query
`/v1/models` and fall back to the actually-loaded model, render a settings template with
the URL injected. We already have `ai-console::lmstudio` code but it is NOT wired into any
manifest recipe or the routing `base_url`. Wire it up: add an `lmstudio-local` provider
recipe (base_url from detection via ProviderDefaults/`{base_url}`), use lmstudio.rs for
detection, and expose it in the launcher's provider list. `_resolve-lmstudio-url.ps1` in
the reference is the detection algorithm to port/confirm against lmstudio.rs.

## Acceptance
- Selecting an agent × lmstudio-local launches against a detected local LM Studio with the
  loaded model, no manual URL entry.
