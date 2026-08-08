---
id: CPE-1438
title: "Crypto 'Inspect'/'Inspect JWT' is a silent no-op in dual-pane mode"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1417
created: 2026-08-07
---
## Bug (found by the CPE-1433 integration sweep)
Right-click a `.jwt`/`.pem`/`.crt`/`.der`/`.csr` file in **either pane while dual-pane is active** → the
"Inspect" / "Inspect JWT" menu row appears and is clickable → clicking it does **nothing visible**.

**Root cause:** the right-hand grid slot in `src/App.svelte` (~L5709) is
`{#if dualPane} <pane B ExplorerPane> {:else if showDetails} <PreviewPane/> {/if}` — so `PreviewPane`
(and thus `JwtPreview`/`CertPreview`) is **never mounted while `dualPane` is true**; pane B occupies that slot.
But `inspectCryptoFile()` (~L2411) only sets `showDetails = true; showPreview = true` — flags with zero effect
in dual-pane — and the menu-gating predicates `certKindOf`/`isJwtFile` (~L1677) gate purely on the selected
file's extension with **no `dualPane` awareness**, so the action is offered where it can't work. (The sibling
Create/Sign cert actions work in dual-pane because they're **modals**, not the inline preview slot — that's why
Inspect is the one action in the epic that got missed. Folder drill-down + `.eml`/`.ics`/`.vcf` previews are
also unavailable in dual-pane by the same architectural fact, but they don't OFFER a dead menu item.)

## Fix — preferred: make Inspect WORK in dual-pane (serves the epic's stated goal)
The user's CPE-1417 goal was explicitly to **manage/inspect certs from the dual-pane right pane**. So the
better fix is to make `inspectCryptoFile()` in dual-pane open the decode in an **overlay/modal** (reuse the
existing `JwtPreview`/`CertPreview` components inside a dialog shell — the same "modal works in dual-pane"
pattern the Create/Sign dialogs already use), instead of relying on the inline preview slot. Full-screen or a
centered dialog with a visible border (dialog convention). Esc/click-outside closes.

**Acceptable fallback** (if the overlay proves too invasive): gate the "Inspect"/"Inspect JWT" menu rows behind
`!dualPane` (hide them when dual-pane is active), matching the codebase's existing "don't offer what can't work"
pattern — but this is strictly worse for the user's dual-pane crypto-management intent, so prefer the overlay.

## Tests
Extend `src/App.paneBCertMenu.test.ts`: with dual-pane active, clicking "Inspect JWT" on a `.jwt` row must
produce the decode (overlay mounts with the parsed fields) — NOT a no-op. Cover both panes + both file kinds
(jwt + cert). Mutation-verify. `npm run check` + `npx vitest run` green.

## Notes
Found composing dual-pane (CPE-677) × crypto inspect (CPE-1424) — each passed in isolation; neither test crossed
both (`App.paneBCertMenu.test.ts` only asserted the menu item's presence, `ContextMenu.test.ts` only the
dispatched action string). Frontend-only, no Rust.
