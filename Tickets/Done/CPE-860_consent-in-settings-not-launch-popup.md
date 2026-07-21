---
id: CPE-860
title: Move Agent Deck capability consent from a launch-time popup into the Settings dialog
type: enhancement
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
---

## Summary
Clicking the **Agent Deck** button currently interrupts with the capability-consent sheet
(`ConsentSheet.svelte`, CPE-296) before the console opens — the user must decide capabilities every time
there's an undecided one. The user finds this launch-time popup unwanted: the console should open
**directly**, and capability consent should be **managed in the Settings dialog** instead.

The Settings dialog already embeds `SidecarManager` (CPE-274), which lists each sidecar and lets the user
**revoke** granted capabilities — but a *denied* capability only shows a static "denied" label with no way
to **grant** it. So consent management is half-present in Settings already; this ticket completes it and
removes the launch-time gate.

## Approach
1. **No launch-time popup.** `openAiConsole()` goes straight to `launchAiConsole()`. To preserve the prior
   default (the old sheet defaulted non-sensitive capabilities ON, sensitive OFF), on first launch silently
   grant the **non-sensitive** requested capabilities and leave sensitive ones (secrets, network) ungranted
   for the user to grant deliberately in Settings. Remove the `ConsentSheet` wiring from App
   (`consentPrompt`, `onConsentDecision`, the render, the import).
2. **Grant in Settings.** In `SidecarManager`, a denied capability gets a **grant** control (not just the
   "denied" text), so every capability can be granted/revoked from Settings → Platform. Reuses the existing
   `sidecar_set_consent` command via `setConsent`.

Security posture is unchanged: sensitive capabilities are still never granted without a deliberate user
action — the action just moves from a launch-time sheet to the Settings panel.

## Acceptance Criteria
- [x] Clicking Agent Deck opens the console with **no** consent popup.
- [x] Sensitive capabilities are not auto-granted; they start denied and can be granted in Settings.
- [x] Settings → Platform (`SidecarManager`) can both **grant** and **revoke** each capability.
- [x] `ConsentSheet` is no longer wired into the launch flow.
- [x] `npm run check` + full suite green; GUI-verified (Deck opens directly; grant/revoke works in Settings).

## Work Log
- 2026-07-21 — Filed from attended session. Branch `cpe-860-consent-in-settings` off main.
- 2026-07-21 — Implemented (frontend-only, reuses existing `sidecar_set_consent`): removed the
  `ConsentSheet` launch wiring from App (`openAiConsole` opens directly, silently granting non-sensitive
  defaults); added a **Grant** control to `SidecarManager` beside the existing Revoke; added
  `mgr.grant`/`mgr.grantTip` across all 12 locales. `ConsentSheet.svelte` retained (still valid + tested
  by the i18n migration guard) but unwired. check clean; 902 tests pass.
- 2026-07-21 — **GUI-verified** in the sidecar dev build: Agent Deck opens directly (no popup); Settings →
  Platform shows Grant/Revoke per capability and toggling works. User confirmed. Closing.
