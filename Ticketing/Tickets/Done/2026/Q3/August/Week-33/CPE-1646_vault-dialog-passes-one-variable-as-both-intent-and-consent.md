---
id: CPE-1646
title: "VaultCreateDialog passes one variable as both intent and consent, re-creating the pattern CPE-1599 exists to prevent"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Raised by the independent Reviewer of CPE-1630 (PR #836), which added a `confirmed` flag to `vault_create` so
the engine refuses to shred originals unless told explicitly. The backend gate is correct and merged. This is
about how the one authorized caller wires it.

## The gap
`VaultCreateDialog.svelte:86` calls:

    commands.vaultCreate(folderPath, dest, passphrase, shredOriginal, shredOriginal)

— the same variable passed as both the *intent* (`shred_original`) and the *consent* (`confirmed`).

Compare the two precedents the reviewer set it against:
- **CPE-1599** (`BatchMediaDialog.svelte:355-360`): `confirmed_overwrite` is set true only via
  `confirmOverwriteJob(job)`, reachable only through a **separate** confirm panel's "Overwrite N files"
  button — a distinct affirmative act, separate from the initial apply.
- **CPE-1611** (`ShredConfirmDialog.svelte:58`): a hardcoded `true`, which is fine there because the entire
  dialog exists for one destructive click; opening it and clicking its only button *is* the distinct act.
- **CPE-1630** (here): `VaultCreateDialog` is a **general-purpose** dialog — passphrase, destination,
  remember-in-keychain, shred — with a single generic "Create vault" button submitting everything. There is
  no affirmative step specific to the shred warning.

It satisfies CPE-1630's literal acceptance criteria and is a strict improvement on the pre-PR state. But it
re-creates, one layer up inside the authorized caller, exactly the "one flag doing double duty" pattern that
CPE-1599 established a separate parameter to avoid.

**The concrete risk is future drift**: any later change that finds another way to set `shredOriginal = true`
— a restored draft, a deep-link default, a copy-paste refactor — sets `confirmed = true` for free, with no
additional gate. Under CPE-1599's shape, that path would still have to go through the separate confirm
handler.

## Fix
Set `confirmed` from a handler bound to the shred warning panel itself, so consent is a genuinely separate
affirmative act rather than an alias of intent — mirroring CPE-1599. Keep it proportionate: this is a
low-frequency, already-warned flow, so a second full modal is likely overkill; an explicit acknowledgement
control within the existing warning panel is probably the right weight.

While there: the reviewer noted neither `VaultCreateDialog` nor `ShredConfirmDialog` uses `$t()` (0 calls
each, against 45 components that do), so both surface raw backend error strings. That is genuine existing
precedent rather than a shortcut introduced by CPE-1630 — but it means a non-English user sees English on the
two most destructive dialogs in the app. Worth folding into CPE-1634's i18n sweep rather than doing here;
note it, don't fix it.

## Acceptance criteria
- `confirmed` can only become true through an act distinct from setting `shredOriginal`; a test proves that
  toggling the checkbox alone does not produce a confirmed call.
- The backend gate still refuses when `confirmed` is false — don't disturb CPE-1630's tests.
- The existing warning copy (which correctly states there is no Recycle Bin and no undo) is preserved or
  improved, not lost in the rewiring.
- No extra friction for the common case of creating a vault **without** the shred option.

**Conflict surface:** `src/lib/components/VaultCreateDialog.svelte` and its test. Small and self-contained.
