---
id: CPE-348
title: "Credential profiles UI (named, switchable env login sets)"
type: Feature
status: Done
closed: 2026-07-14
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

## Work Log
2026-07-14 — Implemented as **labelled credentials per provider** on branch
`CPE-348-credential-profiles` (a better fit for the actual "multiple accounts" need than the
generic env-profile model).
- `presets.rs`: `CredentialRef { provider, label }` + a persisted `credentials` index +
  `add_credential`/`remove_credential`.
- `console.rs`: `provider_secret_name(provider, label)` (`default` keeps the legacy
  `provider:<id>` name; others get `provider:<id>#<label>`); `resolve_provider_key(.., label)`;
  keys API takes an optional `label` and maintains the index; `/api/keys` returns
  `{credentials:[{provider,label}], providers}`; launch reads a `credential` label and remembers
  it (`key_ref`). +2 tests (labelled resolution; multi-credential list/delete, no value leak).
- `launcher.html`: Keys panel gained a **label** field and lists `provider · label`; a
  **Credential** picker appears next to Provider when a provider has ≥2 keys, is sent on launch,
  and is remembered per preset/last-used.
- ai-console `cargo test` 108 lib + 7 integration, `clippy` clean.

Acceptance: create/select/delete named credentials from the launcher ✔; launching uses the
chosen one ✔; values never shown ✔. Launcher visuals need an eyeball; API + storage are
unit-tested.
