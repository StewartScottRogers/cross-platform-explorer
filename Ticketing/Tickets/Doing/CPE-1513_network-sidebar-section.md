---
id: CPE-1513
title: "Network left-pane sidebar section + connections UI (SFTP/WebDAV entry point)"
type: Feature
status: Doing
priority: High
component: Frontend
tags: [ready]
epic: CPE-1498
created: 2026-08-08
---
## What (the visible entry point for the now-browsable SFTP/WebDAV backend)
A new collapsible **Network** section in `src/lib/components/Sidebar.svelte`, matching the existing section
pattern (agents/favorites/tags/smart/savedSearch/explore/places/drives in the persisted `sidebarSections`
store). Hidden/empty when the user has no saved connections and no OS-discovered shares → the plain explorer is
unchanged when unused.

## Scope (this slice)
- New `"network"` key in the `sidebarSections` store + the collapsible header/twisty (reuse the Drives pattern).
- **Two deduped tiers of rows:**
  1. **Saved connections** (from `connections.rs` via a Tauri command — add `connections_list`/`upsert`/`remove`
     wrappers over `connections::load/upsert/remove` if not present) — each with a **state dot**: connected /
     saved-disconnected / error (e.g. host-key changed → distinct error state).
  2. **OS-discovered shares** (from the existing `list_network_shares` / `net_share.rs` command).
- **"＋ Add a connection"** — an inline/instant control ([[prefer-inline-instant-controls]]), NOT a modal:
  protocol dropdown (sftp/webdav to start), host, optional user/port, path. On save, upsert the `Connection`.
- **Connect**: clicking a saved connection navigates into its root remote path (the URI, e.g. `sftp://host/path`)
  — this now WORKS via CPE-1511 (list_dir routes remote). If the connection needs a secret and none is stored,
  prompt for it inline (password/passphrase) with a "remember" toggle → on remember, call CPE-1510's
  `connection_secret_set(name, secret)`.
- **Per-connection context menu** (MENUS.md — theme colours only, leading icons per [[menu-items-need-icons]]):
  Connect / Disconnect · Edit · Forget (Forget also deletes the keychain secret via `connection_secret_delete`).
  ("Mount as drive" is DEFERRED — needs CPE-1500 OS-mount, not built; omit it, don't stub a dead item.)
- Dialogs get a visible border ([[dialogs-need-visible-border]]); path fields a Browse affordance where it
  makes sense ([[path-inputs-need-picker]]); any capability chips reflow ([[tick-tacks-reflow]]).

## Verify
- `npm run check` (type-check) clean; `npm run test:unit` green; add unit tests for the sidebar-section store
  logic + the connection-row state mapping where jsdom-testable.
- **VISUAL / behavioral verification is PENDING** — it needs the gui-smoke Visual Critic (screenshots of the
  real built app) or the user's attended eyes. Do NOT claim the visual result is done. Add a `gui-smoke` spec
  scaffold if practical (a `network.smoke.ts` asserting the section renders), but note the full visual/interaction
  sign-off is outstanding.
- Docs page per CPE-579 (Network section).

## Notes
Frontend-heavy + a thin connections command layer. Leave CPE-1513 in `Doing` (pending visual verification) with a
Work Log; the Foreman will run a gui-smoke Visual Critic pass or hand it to the user. Sits on top of CPE-1510 +
CPE-1511 (both merged).
