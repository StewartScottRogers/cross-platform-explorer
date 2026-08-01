---
id: CPE-1182
title: "Archive password support end-to-end (extract prompt + create-with-password)"
type: feature
component: Frontend
priority: medium
status: Done
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
- 2026-07-31 — Done. Added `passwordPrompt` state + a render block (`App.svelte`) mirroring `confirm`/
  `ConfirmDialog`'s show/dismiss pattern, using `PasswordPromptDialog` (CPE-1179). `doExtract` and
  `enterArchive` share a new `extractWithPasswordFallback`/`promptForExtractPassword` pair: a plain
  `extractArchive` is tried first, and any error containing "password" (the `zip` crate's own wording
  for both "no password given" and "wrong password" — verified in `crates/server/src/archive.rs` /
  the `zip` crate source) opens the dialog and retries via `extractZipEncrypted`; a wrong password
  re-prompts with the `error` prop instead of dismissing; Cancel aborts cleanly. `enterArchive` reuses
  the same fallback for the browse path — since the backend has no password-aware entry LISTER (only a
  password-aware full extract), an encrypted zip that can't be browsed in place extracts to a sibling
  folder instead once the password is given (documented in-code; the closest honest equivalent to
  "opening" it the backend supports). New "Compress with password…" context-menu row → `doCompressWithPassword`
  collects a password via the same dialog (re-prompting on empty) → `compressToZipEncrypted`. New `lock`
  icon glyph (`Icon.svelte`) for the row; i18n keys added to all 12 `COMPLETE_LOCALES` catalogs. Tests:
  `src/App.archivePassword.test.ts` (8 cases: wrong→right password retry, Cancel abort, locked-archive
  enter-and-extract, extract-to, compress-with-password incl. empty-password re-prompt) +
  `ContextMenu.test.ts` new-row cases. `gui-smoke/specs/archive-password.smoke.ts` added — creates a
  GENUINE AES-encrypted zip via the app's own "Compress with password…" (no hand-rolled crypto), then
  extracts it for real, asserting `snap("password-prompt-extract")` + the wrong/right password flow;
  not run locally (CI's job) but `cd gui-smoke && npm run typecheck` passes. Docs: added an "Archives"
  bullet to `src/docs/03-explorer.md`. `npm run check` (0 errors) and `npm test` (134 files / 1512 tests)
  green. Built with CPE-1183 on one branch — see that ticket for its own log.
