---
id: CPE-1182
title: "Archive password support end-to-end (extract prompt + create-with-password)"
type: feature
component: Frontend
priority: medium
status: Doing
tags: ready
created: 2026-07-31
epic: CPE-705
---

## Summary
Part of the CPE-705 GUI remainder. Wire encrypted archives into the UI: prompt for a password when extracting/
entering an encrypted zip, and offer "Compress with password…". Consumes CPE-1179's PasswordPromptDialog.
**Wave 2** (after 1179 lands). Owns the password-dialog state + render block in App.svelte. Build together with
CPE-1183 by one worker (shared `ContextMenu.svelte` + `doCompress`).

## Build
- Extract/enter path: in `doExtract` (`App.svelte:2332`) and `enterArchive` (`:1219`), catch the AES/encryption
  error → show `PasswordPromptDialog` → retry via `extractZipEncrypted`; wrong password re-prompts with the
  `error` prop.
- Create path: add a "Compress with password…" context-menu row → collect password → `compressToZipEncrypted`.
- Route `invoke` via `src/lib/invoke.ts` ([[busy-cursor]]); menu rows theme-only per MENUS.md, with icons
  ([[menu-items-need-icons]]).

## Acceptance Criteria
- [ ] Headless test: build an encrypted-zip fixture in-test via `compressToZipEncrypted`; correct password
      extracts, wrong password surfaces the error + re-prompt; create-with-password yields a zip that only opens
      with that password.
- [ ] gui-smoke `snap("password-prompt-extract")` + the new menu row; `npm run check` + `npm test` green.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-705). Dep: CPE-1179. Batched with CPE-1183 (shared files).
