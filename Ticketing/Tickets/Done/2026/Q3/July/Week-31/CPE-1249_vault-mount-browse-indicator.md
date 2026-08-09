---
id: CPE-1249
title: "Vault mount/browse: unlock → browse the decrypted tree as a location + locked/unlocked tree indicator"
type: Task
priority: Medium
component: Multiple
tags: [ready]
estimate: 3h
created: 2026-08-01
epic: CPE-738
closed:
---

## Context
Third slice of the encrypted-vaults half of CPE-738. Delivers the "mount transparently for browsing" DoD
bullet for CONSUMING an existing vault: a `.cpevault` file shows a lock indicator; unlocking it (passphrase)
decrypts into a session dir the explorer then browses as a normal location; locking wipes it. Builds on the
merged backend commands (CPE-1248): `vaultIs`, `vaultUnlock`, `vaultLock`, `vaultStatus`,
`vaultRememberPassphrase/Forget` + `VaultStatus` — all in `src/lib/bindings.gen.ts`. Creating vaults (the
destructive seal) + Settings + docs are the NEXT slice (CPE-1250) — not here.

## Grep-first (find the real patterns before building)
- How a directory listing / tree row renders an icon + any badge/overlay (e.g. `ThumbnailImage.svelte`,
  the mismatch ⚠ badge, file-type icons). Add the lock indicator the same way.
- How navigating to a path works (the store/function that sets the current folder) — unlock navigates INTO
  the session dir; lock navigates back OUT.
- The existing dialog components (`ShredConfirmDialog.svelte`, `BatchRenameDialog`, `ColumnPickerDialog`)
  and their conventions — the unlock passphrase dialog MUST match: a clearly-visible thin border
  ([[dialogs-need-visible-border]]), theme-only colours, item text `var(--text)`, focus-trap/Esc-close if
  the others do it.
- `invoke` MUST come from `src/lib/invoke.ts` (busy-cursor wrapper), never `@tauri-apps/api/core`
  ([[busy-cursor]] convention).
- `@tauri-apps/api/path` for a session-dir base (see "session dir" below).

## What to build (mostly frontend + a tiny glue)
1. **Vault store** (`src/lib/vaultStore.ts`): tracks unlocked vaults (blob path → session dir) + derives
   status. Reuses `vaultStatus`/`vaultUnlock`/`vaultLock`.
2. **Tree/listing indicator:** a `.cpevault` file renders a **locked** 🔒 badge; when unlocked (per the
   store) a **unlocked** 🔓 badge. Reuse an existing `Icon` glyph (per [[menu-items-need-icons]] / the
   badge pattern) — do NOT hard-code colours; theme variables only. Badging by the `.cpevault` extension
   for the glyph is fine (there is no bulk `is_vault`); confirm a candidate via `vaultIs` on interaction.
3. **Unlock flow:** activating a locked `.cpevault` (double-click / a "Unlock vault…" context action) opens
   a **passphrase dialog** (functional, following the dialog conventions above; a password `<input>`;
   Enter submits, Esc cancels; show a clear error on `BadPassphrase` — distinct copy from `Corrupt`
   "vault file is damaged"). On success: call `vaultUnlock(blob, passphrase, sessionDir)` then navigate
   INTO `sessionDir`. Show a header/banner while browsing an unlocked vault: e.g. "🔓 <vaultname> —
   unlocked" with a **Lock** button.
   - **Session dir:** compute a unique app-private path — `await appCacheDir()` + a `crypto.randomUUID()`
     subdir (unpredictable, user-private). Pass it to `vaultUnlock`. Document (in a code comment) that while
     unlocked the plaintext lives there on disk — the mount tradeoff the epic accepts for v1; `vaultLock`
     wipes it.
4. **Lock flow:** the Lock button / a "Lock vault" action → navigate OUT of the session dir first (so the
   view isn't sitting in a dir about to be wiped) → `vaultLock(blob)` (backend wipes the session dir) →
   update the store → badge returns to 🔒.
5. If unlock fails, do NOT leave a half-open state (no stale store entry, no navigation).

## Acceptance criteria
- **vitest** (`src/lib/vaultStore.test.ts` + any component logic): unlock records state + session dir;
  lock clears it; a failed unlock leaves no state; the badge state derives correctly from the store.
- **gui-smoke** (`gui-smoke/specs/vault.smoke.ts`): with a **seeded `.cpevault` fixture** (create it in
  `wdio.conf.ts#onPrepare` by calling the backend/`age` or by shipping a pre-made blob + known passphrase),
  drive the real built app: locate the vault row (🔒 badge visible), open the unlock dialog via its in-app
  trigger, type the passphrase, and assert the decrypted tree becomes browsable (a known inner file row
  appears), then Lock and assert the view returns and the badge is 🔒 again. Capture `snap("vault")` for
  the Visual Critic; `snapFailure` on failure (follow the spotlight/column-picker spec conventions).
- `npm run check` clean; `npm run test` (vitest) green; gui-smoke green locally if runnable (note if the
  harness needs the stalled CI runners).
- Dialog matches conventions (visible border, theme colours, `var(--text)`); no hard-coded colours; pills/
  badges follow the reflow rule if any pill row is added.

## Out of scope (CPE-1250)
Create-vault flow + the destructive seal-and-shred confirm; Settings keychain-caching toggle;
remember-passphrase checkbox UX; in-app docs page; final visual polish pass.

## Notes
Keep the passphrase in memory only; never log it. If `vaultRememberPassphrase` is wired for convenience,
that's fine, but the full "remember in keychain" checkbox UX belongs to CPE-1250 — a minimal or no
remember here is acceptable.

## Done 2026-08-02 (sprint) — merged #552 @ f2d53089
Unlock a .cpevault via a passphrase dialog → browse the decrypted tree as a location; lock wipes it.
VaultBadge 🔒/🔓 + VaultBanner + vaultStore + app-cache session dir. Full gauntlet (this slice was the
hardest): Reviewer APPROVE after fixing re-unlock plaintext-orphan (#1, frontend guard + backend
best-effort superseded-wipe), failed-lock retry-reachability (#2), dialog remount (#3), vaultIs-error
handling (#4); UAT on the REAL build caught a Lock button rendering off-viewport (app #app grid `auto`
column sized to the deep session path → horizontal overflow) → fixed with `#app{minmax(0,1fr)}` +
`.address{min-width:0;overflow-x:auto}` + `scrollAddressToStart` (Home crumb stays clickable); Visual
PASS; and an INDEPENDENT full-suite gui-smoke regression gate that caught a fixture-pollution red herring
(vault fixture added a top-level folder, shifting other specs' rows below the fold) — fixed (fixture is a
root-level file), then independently re-confirmed all 5 gate specs green. Follow-ups filed: CPE-1252
(orphan-session sweep), CPE-1253/1254 (pre-existing non-vault bugs the gate surfaced).
