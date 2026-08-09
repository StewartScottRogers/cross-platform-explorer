---
id: CPE-1516
title: "Promote Network to a permanent top-level left-pane section (like Drives/Quick Access)"
type: Feature
status: Done
priority: High
component: Frontend
tags: [ready]
epic: CPE-1498
created: 2026-08-09
---
## Why (user feedback, 2026-08-09)
The user expected **Network** to be its own first-class left-pane section — peer to **Tags, Explore, Quick
Access, and Drives** — but couldn't find it, because today it only appears *conditionally*. CPE-1513 shipped
Network as a collapsible section that is **hidden until there's a saved connection or an OS-discovered share**,
mirroring Favorites/Tags/Smart; before then, the only entry point is a small **"Network…" row buried inside the
Explore section** (`Sidebar.svelte:433-446`). That buried, easy-to-miss affordance is the disconnect: a user
with no connections yet sees no Network section at all.

## What
Make **Network** a stable, always-present, labelled top-level section so it's discoverable before the first
connection exists — the same mental model as Drives (always shown, even with one drive).

## Scope
- **Always render the Network section header** (the `fav-title` "Network" heading + twisty), regardless of
  whether there are rows yet — do NOT gate the whole section on `hasAnyNetworkRows(...)`. When there are no
  rows, the section body shows the **"＋ Add a connection"** control (and a one-line empty hint), so adding the
  first connection happens *in* the Network section, not via the Explore row.
- **Remove the "Network…" row from the Explore section** (`Sidebar.svelte:433-446`) — it exists only because
  the section could be absent; once the section is permanent, the row is redundant. (Keep the shared
  `openAddConnection` handler; just move its trigger into the Network header/body.)
- **Ordering:** place Network as a sibling near Drives — proposed order Favorites → Tags → Smart Folders →
  Saved Searches → Explore → Quick Access → **Drives → Network** (Network last, adjacent to Drives, since both
  are "volumes/locations"). Respect the persisted `sidebarSections` store (add/confirm the `"network"` key and
  its default position + collapse state).
- **Preserve the additive-mode guarantee** (CLAUDE.md): with no connections and no shares, the permanent
  Network section must stay visually quiet (header + "＋ Add" + hint only) — it must NOT make the plain
  explorer feel heavier. Confirm the empty state is unobtrusive.
- Keep the two deduped tiers (saved connections; OS-discovered shares) exactly as CPE-1513 built them.

## Verify
- `npm run check` clean; unit tests for the section-visibility change (the section renders with zero rows;
  the Explore "Network…" row is gone; ordering/default store state).
- **Attended/visual sign-off** (with the user, or gui-smoke Visual Critic screenshots): the empty Network
  section reads as a peer of Drives, the "＋ Add a connection" is obvious, and the plain explorer still feels
  light. Pairs with the pending CPE-1513 visual verification.
- Update the `src/docs/31-network.md` page (CPE-579) to describe Network as a permanent section.

## Notes
Small, frontend-only, testable without a NAS. Spun out of the CPE-1513 visual-verification feedback. Sits on
top of the merged CPE-1513 code. Relates to [[prefer-inline-instant-controls]] (the ＋ Add control),
[[menu-items-need-icons]], the TABS/section conventions.

## Work Log (2026-08-09)
- `src/lib/components/Sidebar.svelte`: the Network section header + body now always render (no longer
  gated on `hasAnyNetworkRows(...)`); the standalone Explore "Network…" row (`Sidebar.svelte:433-446`) was
  removed. When the section has no saved connections/shares, its body shows a "＋ Add a connection" row
  (same `networkAdd` dispatch, same title text the old Explore row used) plus a one-line "No connections
  yet — add an SFTP or WebDAV server." hint — header + control + hint only, so the plain explorer stays
  visually quiet per CLAUDE.md's additive-mode guarantee.
- Ordering was already Network-after-Drives in the DOM (the section markup sits right after the
  places/drives loop), which matches the ticket's proposed order — no reordering needed.
- `src/lib/sidebarSections.ts` is a generic id→open map with no fixed section list or explicit ordering
  concept (`isOpen` already defaults any unset id, including `"network"`, to open) — confirmed no code
  change was needed there; DOM order is what drives visual ordering.
- `src/lib/network.ts`: updated `hasAnyNetworkRows`'s doc comment — it now gates the section's *body*
  content (rows vs. empty-state), not the section's visibility.
- Docs: `src/docs/31-network.md` rewritten to describe Network as a permanent section (peer of Drives) and
  the "＋ Add a connection" row as living inside the section itself.
- `gui-smoke/specs/network.smoke.ts`: updated comments/assertions for the permanent header; the empty-state
  button kept its original title text ("Add a saved SFTP/WebDAV connection") so the existing selector still
  matches after the move.
- Tests: added 6 new cases to `src/lib/components/Sidebar.test.ts` covering the always-rendered header, the
  empty-state control + hint, the `networkAdd` dispatch from it, the removed Explore row, and that rows
  replace the empty state once connections/shares exist. `npm run check` — 0 errors/warnings. `npx vitest
  run` on `network.test.ts` + `Sidebar.test.ts` + `sectionDocs.test.ts` — 48/48 passing.
- **Visual/interaction sign-off is OWED to the user** (attended check or gui-smoke Visual Critic
  screenshots) — not claimed done here, per the ticket's Verify section. Pairs with the still-pending
  CPE-1513 visual verification.
