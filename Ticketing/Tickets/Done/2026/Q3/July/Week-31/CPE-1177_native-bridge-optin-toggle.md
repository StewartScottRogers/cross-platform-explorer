---
id: CPE-1177
title: "Native-bridge opt-in toggle (nativeBridgeEnabled) + gate TagEditor native controls"
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-31
epic: CPE-717
---

## Summary
Part of the CPE-717 GUI remainder. Introduce the persisted **`nativeBridgeEnabled`** setting (default `false`),
add a toggle for it in SettingsDialog, and gate the existing `TagEditor` native pull/push controls (currently
always visible) behind it. **This ticket OWNS the setting key** — CPE-1176 consumes it. Build together with
CPE-1176 on one branch (same worker) to avoid a cross-branch key race.

## Build
- Add `nativeBridgeEnabled: boolean` (default `false`) to `src/lib/settings.ts` (persisted, round-trips).
- Add a labelled toggle in `src/lib/components/SettingsDialog.svelte` (follow the existing settings-toggle
  pattern; avoid launch-time permission modals — this is a Settings control per [[avoid-modal-permission-popups]]).
- Gate `src/lib/components/TagEditor.svelte`'s native pull/push buttons behind `nativeBridgeEnabled` (hidden when
  off).

## Acceptance Criteria
- [ ] `settings.test.ts`: `nativeBridgeEnabled` defaults off and round-trips through persist.
- [ ] gui-smoke (or jsdom) shows the toggle in SettingsDialog; TagEditor native buttons hidden when off, shown
      when on.
- [ ] `npm run check` green.

## Work Log
- 2026-07-31 — Filed by Foreman (sprint, epic CPE-717). Owns `nativeBridgeEnabled`. Built with CPE-1176 by one worker.
- 2026-07-31 — Done. Added `nativeBridgeEnabled: boolean` (default `false`) to `src/lib/settings.ts`
  (persisted via the standard `read`/`write` helpers, round-trips through `settings.json`). Added a
  labelled toggle + description in `SettingsDialog.svelte` ("Native metadata bridge" section, self-contained
  like `ShellIntegration.svelte` — reads/writes `settings.ts` directly, no launch-time modal). Gated
  `TagEditor.svelte`'s native pull/push controls (+ the `pullNative`/`pushNative` handlers, defense-in-depth)
  behind the flag — hidden when off, shown when on, still per-path-only (hidden in batch mode regardless).
  Tests: `settings.test.ts` (`nativeBridgeEnabled` defaults off + round-trips) and new `TagEditor.test.ts`
  (hidden when off, shown when on, hidden in batch even when on). `npm run check` 0 errors; `npm test` all
  green. Built together with CPE-1176 on branch `cpe-1177-1176-native-metadata-gui`, PR opened against `main`.
