---
id: CPE-1498
title: "EPIC: Network F2 — 'Network' left-pane section + connections UI"
type: Task
status: Proposed
priority: Medium
component: Frontend
tags: [epic]
epic: CPE-616
created: 2026-08-08
closed:
---

> **Network Filesharing program (parent CPE-616 / reconciles CPE-716). Foundation epic F2.** Filed 2026-08-08
> (sprint PM, Network research — research-library `network-filesharing-program-2026-08-08`). Dormant.

## Why (the user-visible deliverable: a "Network" section in the left pane)
`Sidebar.svelte` has sections (agents/favorites/tags/smart/savedSearch/explore/places/drives) in one persisted
`sidebarSections` store — **there is no Network section**. A new `"network"` key slots straight into that
pattern. Hidden/empty when the user has no connections and no OS-mounted shares → plain explorer unchanged.

## Scope
- New collapsible **Network** section (same header/twisty pattern as Drives). Two deduped tiers of rows:
  1. **Saved connections** (from `connections.json`) with a **state dot** (connected / saved-disconnected /
     mounted-as-drive / error e.g. host-key changed → loud).
  2. **OS-discovered shares** (from the existing `list_network_shares` / `net_share.rs`).
- **"＋ Add a connection"** — inline/instant control ([[prefer-inline-instant-controls]]), NOT a modal: protocol
  dropdown + host + optional user; auth prompted at **connect** time (not launch-time consent —
  [[avoid-modal-permission-popups]]); path fields get Browse ([[path-inputs-need-picker]]); dialogs have a
  visible border ([[dialogs-need-visible-border]]).
- Per-connection **context menu** (MENUS.md — theme colours only, leading icons [[menu-items-need-icons]]):
  Connect/Disconnect · Mount as drive/Unmount (the hybrid switch, → CPE-1500) · Edit · Forget (also deletes the
  keychain secret). Capability chips reflow per [[tick-tacks-reflow]].
- Tauri command wrappers over `connections::load/upsert/remove`.

## Effort / deps / fit
M (frontend-heavy) + thin command layer. Deps: CPE-1497 (to actually connect). Additive, hidden when unused.
Ship docs per CPE-579.
