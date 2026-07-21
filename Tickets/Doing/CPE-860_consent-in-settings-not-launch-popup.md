---
id: CPE-860
title: Move Agent Deck capability consent from a launch-time popup into the Settings dialog
type: enhancement
component: Frontend
priority: medium
status: In Progress
tags: ready
created: 2026-07-21
closed:
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
- [ ] Clicking Agent Deck opens the console with **no** consent popup.
- [ ] Sensitive capabilities are not auto-granted; they start denied and can be granted in Settings.
- [ ] Settings → Platform (`SidecarManager`) can both **grant** and **revoke** each capability.
- [ ] `ConsentSheet` is no longer wired into the launch flow.
- [ ] `npm run check` + full suite green; GUI-verified (Deck opens directly; grant/revoke works in Settings).

## Work Log
- 2026-07-21 — Filed from attended session. Branch `cpe-860-consent-in-settings` off main.
