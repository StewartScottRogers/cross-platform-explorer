---
id: CPE-246
title: Design a reasonable middle-pane context menu feature set (vs Windows Explorer / macOS Finder)
type: Task
status: Done
priority: Medium
component: Frontend
estimate: 1h (design) + impl TBD
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Think through the middle-pane right-click context menu and design a reasonable
feature set, informed by what Windows File Explorer and macOS Finder offer.
Deliverable (this ticket): the design/proposal. Implementation of any chosen
additions will follow as their own tickets.

## Acceptance Criteria
- [ ] Documented comparison of Windows Explorer + Finder context menus.
- [ ] Proposed feature set for this app: item menu + empty-space menu.
- [ ] Marks what already exists vs new; flags what needs backend support.
- [ ] User picks the subset to implement (each becomes its own ticket).

## Resolution / Design

Compared Windows 11 Explorer + macOS Finder context menus against the app's
current menu. Current menu already covers the core (Cut/Copy/Rename/Delete, Open,
Execute/Execute-as-admin, Open in new tab, Duplicate, Copy as path, Properties;
empty: New folder/Paste/Select all/Refresh).

Proposed additions, by effort:
- Quick wins (reuse existing capabilities): Reveal in File Explorer/Finder
  (`revealItemInDir`), Pin to Home / Unpin (existing pins), Copy name, Open in
  new window (WebviewWindow), Sort by ▸ / View ▸ in the empty menu, pop-out
  preview.
- Medium (new backend): Open with…, Open in Terminal, New ▸ (files),
  Compress to ZIP, Extract here/to.
- Skipped: Share sheet, Tags, Make Alias, Send to.

Recommendation: do the quick wins first. Per the ticket-first rule each chosen
item gets its own ticket (CPE-247 Reveal, CPE-248 Copy name, CPE-249 Pin to Home,
CPE-250 Open in new window implemented under "complete all the tickets").

## Work Log

2026-07-12 — Delivered the design/comparison + prioritized proposal. Quick-win items split into their own tickets for implementation.
