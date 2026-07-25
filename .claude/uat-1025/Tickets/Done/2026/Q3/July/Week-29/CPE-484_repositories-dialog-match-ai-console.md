---
id: CPE-484
title: "Improve the Repositories dialog to look + work like the AI Console"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
created: 2026-07-15
closed: 2026-07-15
estimate: 2-3h
epic: CPE-429
---

## Summary
The Repositories dialog (`RepoBrowser.svelte`) is functional but plain compared to the AI Console
launcher. Bring it up to the same standard: the same **labeled toolbar** aesthetic (uppercase field
labels, 30px rounded inputs, a primary accent action button), a unified **status message line**, a
cleaner browser body + breadcrumbs, and a **status bar** — a polished mini-app, not a bare form.

## Acceptance Criteria
- [x] Toolbar mirrors the AI Console: labeled fields (PROVIDER / REPOSITORY / TOKEN) in a subtle-grey
      toolbar with a bottom border; inputs 30px, 6px radius; a primary **Browse** button + **Clone**.
- [x] A single status line (loading / clone progress / errors) like the launcher's `#msg`.
- [x] Header bar with title + close; a bottom status bar showing the current repo · path.
- [x] Breadcrumbs + file list restyled to match; clear empty/loading/error states.
- [x] Uses the explorer theme tokens; legible in the light theme; keyboard-friendly (Enter = Browse).
- [x] `npm run check` clean; existing behaviour (browse/clone/token-remember/up-nav) preserved.

## Notes
Requested by the user: "improve the Repositories dialog so it works as well as the AI Console … look
the same. Improve everything about it." Part of the forge epic ([[CPE-429]]).

## Resolution
Redesigned `RepoBrowser.svelte` to mirror the AI Console launcher, keeping all backend behaviour
(browse / clone / token-remember / up-nav) intact:
- **Window chrome:** a titlebar (icon + "Repositories" + hover-highlighted ×), and a bottom **status
  bar** showing the current repo · path — a mini-app, not a bare modal. Fixed 760×620 panel.
- **Toolbar** matches the launcher: a subtle-grey band with **labeled uppercase fields**
  (PROVIDER / REPOSITORY / TOKEN), 30px 6px-radius inputs that focus to the accent colour, a **primary
  accent Browse** button + a secondary **Clone**, and the Remember-token checkbox on its own line.
- **Unified status line** (the launcher's `#msg` equivalent): one line that shows errors (red), clone
  progress (accent), loading, or a resting item-count / hint — instead of scattered messages. The list
  body no longer duplicates the error text (it points at the status line).
- **Breadcrumbs + file rows** restyled with the explorer theme tokens; clearer empty/loading states;
  Enter in the Repository field triggers Browse.

Files: `src/lib/components/RepoBrowser.svelte`. Verified: `npm run check` clean; **458 frontend tests
pass** (the 7 RepoBrowser tests updated selectors-free — token placeholder kept "token", errors no
longer duplicated).
