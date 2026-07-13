---
id: CPE-296
title: Capability consent & permission UX
type: Feature
status: Done
closed: 2026-07-13
priority: High
component: Multiple
estimate: 2-3h
created: 2026-07-13
---

## Summary

"No ambient authority" needs a human in the loop. When a sidecar first requests
capabilities (context, secrets, storage, events, network), the user sees a clear
consent prompt — what it wants and why — and grants or denies per capability. The
broker ([[CPE-266]]) enforces the decision; grants are revocable later.

## Acceptance Criteria

- [ ] First-run consent sheet per sidecar listing each requested capability with a
      plain-language description and risk note (esp. secrets).
- [ ] Per-capability grant/deny; denial degrades the sidecar gracefully, never
      crashes it.
- [ ] Grants persisted, viewable and **revocable** in the management UI ([[CPE-274]]).
- [ ] A new capability requested after an update re-prompts for just that one.
- [ ] Tests: deny secrets → sidecar told, no secret access possible.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-266]]. **Phase:** P3. **Epic:** [[CPE-260]]. Pairs with
[[CPE-295]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-13 — **Implemented.** Backend: `sidecar-host::consent::ConsentStore` persists
per-sidecar grant/deny decisions to `consent.json` (granted/denied/undecided; `record`,
`revoke`, `granted`, `undecided`) — 4 unit tests incl. *deny-secrets → not granted* and
*granted feeds `decide_grants` to exclude denied*. Tauri commands (feature-gated):
`sidecar_consent_state`, `sidecar_set_consent`, `sidecar_revoke_capability`;
`sidecar_start_ai_console` now grants only the persisted consented set (was hardcoded to
all four), so anything unconsented is withheld and the sidecar degrades gracefully (still
serves its UI; broker denies capability calls). Frontend: `ConsentSheet.svelte` — per-cap
grant/deny with plain-language description + sensitive-risk badge (secrets/network default
off), Escape/backdrop to cancel; `openAiConsole` prompts only when there are *undecided*
caps (a new cap after an update re-prompts just that one), persists via `setConsent`, then
launches. Client helpers + `CAPABILITY_INFO` in `sidecar.ts`.

Acceptance: (1) first-run sheet ✅, (2) per-cap grant/deny + graceful denial ✅, (3) grants
persisted + revoke wired ✅ — the *viewing/revoking surface* lands with the management UI
(CPE-274), (4) new-cap re-prompt ✅, (5) deny-secrets test ✅. Verified: host 76 tests,
feature clippy clean, `npm run check` 0/0, 264 frontend tests. **Visual flow of the sheet
still wants a human confirm** (pixels), but the security-critical enforcement is
backend-tested. This is the consent half of the CPE-304 sign-off gate.
