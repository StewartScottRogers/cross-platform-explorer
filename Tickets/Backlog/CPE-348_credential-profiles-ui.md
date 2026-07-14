---
id: CPE-348
title: "Credential profiles UI (named, switchable env login sets)"
type: Feature
status: Open
priority: Low
component: Frontend
created: 2026-07-13
---

## Summary

CPE-287 delivered per-provider key management. The vault also models **credential profiles**
(`vault::ProfileSet` / `CredentialProfile` — named sets of ENV_VAR → vault-key references, so
a user can switch between e.g. "work" and "personal" logins). Expose that in the launcher:
create/edit/delete profiles, pick one at launch, and resolve its env via `vault::resolve_env`
into the session. Backend model + `resolve_env` already exist and are tested; this is the UI +
a console API (`GET/POST/DELETE /api/profiles`, launch param `profile`).

## Acceptance
- Create/select/delete a named profile from the launcher; launching with it injects that
  profile's env; secret values are never shown.
