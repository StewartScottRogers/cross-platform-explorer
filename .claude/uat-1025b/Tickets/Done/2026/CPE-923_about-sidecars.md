---
id: CPE-923
title: About dialog should list all the sidecars
type: feature
component: Frontend
priority: medium
tags: ready
created: 2026-07-23
status: Done
---

## Summary
The About dialog shows only the app name/version/docs link. Add a **Sidecars** section listing every
registered sidecar (name, version, contract, and a health status) from `sidecar_details`, so users can see
what bundled mega-features ship and at what versions. On the plain (sidecar-free) build the section is
omitted (the call returns `[]`).

## Acceptance Criteria
- [x] About dialog lists each sidecar: name, version, contract version, status pill.
- [x] Section hidden when there are no sidecars (plain build).
- [x] Theme-only colours; `npm run check` passes.

## Work Log
- 2026-07-23 — Filed + started.

- 2026-07-23 — About dialog loads sidecar_details() onMount and lists each sidecar (name, version, contract, health pill); section hidden when empty (plain build). check clean.
