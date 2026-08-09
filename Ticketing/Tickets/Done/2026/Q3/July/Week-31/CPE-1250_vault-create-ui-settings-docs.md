---
id: CPE-1250
title: "Vault create UI: seal-a-folder action + destructive confirm + remember-in-keychain + Settings + docs"
type: Task
priority: Medium
component: Multiple
tags: [ready, security-sensitive]
estimate: 3h
created: 2026-08-02
epic: CPE-738
closed:
---

## Context
Fourth slice of the encrypted-vaults half of CPE-738 — the CREATE side (1247 crypto, 1248 lifecycle/
commands, 1249 unlock/browse/lock are merged). Lets a user turn a folder into a `.cpevault`. This is the
DESTRUCTIVE path (optional secure-delete of the original after sealing), so the confirm UX is the
safeguard — get it right.

Backend already exists (CPE-1248): `vaultCreate(folder, destBlobPath, passphrase, opts)` with
`opts.shredOriginal` (default false) + the verify-before-shred + seal⟺extract-symmetry invariants;
`vaultRememberPassphrase`/`vaultForgetPassphrase`. All in bindings.gen.ts.

## Grep-first
- The destructive-confirm honesty patterns + visible border: `ShredConfirmDialog.svelte` (CPE-1240) — reuse
  its tone (PERMANENT / non-recoverable / honest platform caveats) for the "securely delete original" case.
- Passphrase entry: `PasswordPromptDialog.svelte` (used by CPE-1249) — the create dialog needs a passphrase
  + a CONFIRM-passphrase field (must match) since there's no recovery if mistyped.
- Where folder context-menu actions are defined (the same menu the "Securely delete…" / archive actions
  live in) + [docs/design/MENUS.md] + [[menu-items-need-icons]] (leading icon, theme colours, text
  var(--text), never red).
- Settings structure (where toggles live) — put the keychain-caching preference there, NOT a launch-time
  modal ([[avoid-modal-permission-popups]]).
- In-app docs: add `src/docs/vaults.md` AND register the section→slug in `src/lib/sectionDocs.ts` — the
  guard test `sectionDocs.test.ts` fails CI otherwise ([[maintain-in-app-docs-library]]).
- `invoke` only via `src/lib/invoke.ts`.

## What to build
1. **"Create encrypted vault…" folder action** (context menu on a folder) → a **create dialog** (visible
   border, theme colours):
   - Passphrase + Confirm-passphrase fields (must match; show a clear inline mismatch error). Warn that a
     forgotten passphrase means the data is unrecoverable (that's the design).
   - Destination: default `<foldername>.cpevault` as a SIBLING of the folder (the backend REJECTS a dest
     inside the folder when shredding). Allow the user to see/adjust it; offer a native picker per
     [[path-inputs-need-picker]] if a path field is shown.
   - Checkbox **"Securely delete the original folder after sealing"** — default OFF. When ON, the dialog
     shows the honest destructive warning (reuse ShredConfirmDialog's copy: permanent / best-effort on
     SSD/CoW). The backend only shreds after verifying the vault decrypts.
   - Checkbox **"Remember passphrase in this device's keychain"** — default per the Settings preference.
   - On confirm → `vaultCreate(...)`; on success show the new `.cpevault` (navigate to / select it, badge
     locked); if remember checked, `vaultRememberPassphrase`. Surface `VaultError` clearly (e.g. the
     dest-inside-folder refusal, an unsealable-filename refusal — CPE-1247/1248 return `Format` naming it).
2. **Settings toggle:** "Remember vault passphrases in the OS keychain" (drives the create dialog's default
   + whether CPE-1249's unlock offers to use a stored passphrase). Optionally: wire CPE-1249's unlock to
   auto-use a stored passphrase when present (a small enhancement — if quick; else leave unlock as-is).
3. **In-app docs:** `src/docs/vaults.md` — what a vault is, how to create/unlock/lock, the passphrase +
   keychain behavior, and the HONEST limits (plaintext lives in a session dir while unlocked; forgotten
   passphrase = unrecoverable; secure-delete is best-effort on modern storage). Register its section→slug.

## Acceptance criteria
- **vitest**: create-dialog logic (passphrase match/mismatch, default dest = sibling, shred-checkbox gating
  the warning, remember-checkbox); the sectionDocs guard passes.
- **gui-smoke** (`gui-smoke/specs/vault-create.smoke.ts`): from a seeded folder fixture, open the create
  action, enter matching passphrases (shred-original OFF — do NOT destroy fixtures in CI), confirm, and
  assert a `.cpevault` appears with the locked badge. Capture `snap("vault-create")` + `snapFailure`.
  **IMPORTANT — do not pollute other specs:** seed any fixture as a self-contained item that does NOT
  change the top-level folder/file sort order other specs depend on (the CPE-1249 gate was tripped by a
  stray top-level folder — learn from it); prefer a dedicated subfolder or clean up after. Run the FULL
  gui-smoke suite (not just your spec) to confirm no regression before claiming done.
- `npm run check` + `npm run test` green; clippy if any Rust touched (shouldn't be — backend exists).
- Dialogs: visible border, theme colours, text var(--text), NEVER red for the destructive action text
  (colour comes from theme; the WARNING copy conveys severity) per MENUS.md + [[menu-design-standard]].
- No app-wide CSS/layout changes unless truly required (and if so, full-suite gui-smoke verify).

## Out of scope
The orphan-session sweep (CPE-1252); the crew security-review doc (CPE-1251, next slice).

## Notes
Destructive default OFF; verify-before-shred is already backend-enforced but the confirm is the human
safeguard. Passphrase kept in memory only (+ keychain if the user opts in); never logged.

## Done 2026-08-02 (sprint) — merged #553 @ 22860da5
"Create encrypted vault…" folder action + VaultCreateDialog (passphrase+confirm w/ per-field show-hide,
sibling <name>.cpevault dest with native picker, default-OFF secure-delete-original behind an honest
warning, remember-in-keychain) + Settings toggle + in-app docs (src/docs/20-vaults.md + sectionDocs).
Full gauntlet: Reviewer APPROVE (destructive defaults OFF, sibling dest never-inside-folder, vaultCreate
binding correct, docs guard passes, NO app-wide CSS), UAT PASS on the real build, FULL-suite gui-smoke
NO-REGRESSION (the fixture nested to keep the tmpDir root listing byte-identical — slice-3 lesson applied),
Visual PASS after one fix round (both passphrase fields now share a consistent app-owned eye toggle;
native ::-ms-reveal suppressed). CI stalled at merge; verified locally.
