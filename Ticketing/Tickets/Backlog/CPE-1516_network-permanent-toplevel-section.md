---
id: CPE-1516
title: "Promote Network to a permanent top-level left-pane section (like Drives/Quick Access)"
type: Feature
status: Backlog
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
