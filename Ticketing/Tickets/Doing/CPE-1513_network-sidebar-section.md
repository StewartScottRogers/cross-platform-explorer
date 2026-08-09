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

## Work Log (2026-08-09)

**Built:**

- **Rust — `connections_list`/`connections_upsert`/`connections_remove`** (`src-tauri/src/lib.rs`, near the
  existing `connection_secret_*` commands): thin async `spawn_blocking` wrappers over
  `cpe_server::connections::{load_connections, upsert, remove, save_connections}`. Each mutator re-loads
  from `default_connections_path()`, applies the pure reducer, saves, and returns the fresh whole list
  (same "return the updated store" shape as `set_tags`/`rename_tag`). Registered in both
  `generate_handler!` and `collect_commands!`. `cargo build --lib`, `cargo clippy --all-targets -D
  warnings` (default features) and `cargo clippy --all-targets --features sidecar-platform -D warnings`
  both clean. `bindings.gen.ts` regenerated via `cargo run --bin export_bindings --features
  "specta-bindings sidecar-platform"` — adds `Connection`/`AuthMethod` types + the three typed client
  methods. No new Cargo dependency, so `Cargo.lock` is unchanged.

- **`src/lib/network.ts`** (new) — the DOM/IO-free pure logic: `connectionLocation` (mirrors Rust's
  `Connection::location()`), `secretAlwaysRequired`, `stateOf`/`stateTitle` (connect-state → tooltip),
  `isDuplicateShare`/`dedupeShares` (tier-2 OS-share dedup against tier-1 saved connections),
  `hasAnyNetworkRows` (the section's hidden-when-empty gate), and the add/edit form's
  `buildConnection`/`blankConnectionForm`/`formFromConnection`. 27 unit tests in `network.test.ts`, all
  green — this is the "connection-row state mapping" + "sidebar-section store logic" unit coverage the
  ticket asked for (the `sidebarSections` store itself needed no changes: `"network"` is just another
  string id through the existing generic `isOpen`/`toggleSection`, already covered by
  `sidebarSections.test.ts`).

- **`src/lib/components/Sidebar.svelte`** — new collapsible **Network** section mirroring the Drives
  pattern (header + twisty + `nav-children`, persisted via `sidebarSections["network"]`): tier 1 = saved
  connections with a status dot (green connected / gray saved-disconnected / red error, tooltip via
  `stateTitle`); tier 2 = OS-discovered shares, deduped. Right-click a connection dispatches
  `networkContext`; clicking dispatches `networkConnect`; the header's "+" and a new **always-visible
  "Network…" row in the Explore section** both dispatch `networkAdd`.
  **Judgment call (flagging for the Reviewer):** the ticket says the section is "hidden/empty when no
  saved connections and no OS-discovered shares" — taken literally, that would make the section's own
  "+ Add a connection" control unreachable for a first-time user (nothing to click before the section
  exists). Resolved by keeping the collapsible section itself gated exactly as specified, but adding the
  small permanent "Network…" row under Explore (same footprint as Repositories/Agent Board/Workbench) as
  the actual first-connection entry point; once a connection is saved (or a share is discovered) that row
  disappears and the real section + its own "+" take over. Worth a second opinion once the visual lands.

- **`src/lib/components/NetworkConnectionForm.svelte`** (new) — the inline/instant add+edit popover
  ([[prefer-inline-instant-controls]], [[dialogs-need-visible-border]]): name, protocol (sftp/webdav),
  host, optional user/port/remote path, and a password-vs-key-file auth choice with a native Browse
  picker for the key file ([[path-inputs-need-picker]]). Delegates all validation to
  `network.ts#buildConnection` and shows its returned error string inline.

- **`src/lib/components/NetworkConnectionMenu.svelte`** (new) — the row context menu (MENUS.md: `.ctx`
  pattern, theme-only `--text`, no red, leading icons): Connect/Disconnect (whichever applies) · Edit… ·
  Forget. "Mount as drive" intentionally omitted (needs CPE-1500, not built).

- **`src/lib/components/NetworkSecretPrompt.svelte`** (new) — inline password/passphrase popover with a
  "Remember" toggle. **Backend limitation, documented in the component + App.svelte:** there is no
  ephemeral/session-only credential channel yet — the remote route reads a connection's secret from the
  OS keychain, not from the navigate call (CPE-1499 F1 scope). So "Remember" is implemented as "persists
  past this app session": `submitNetworkSecret` (App.svelte) always stashes the secret in the keychain
  long enough for the connect to succeed, then immediately deletes it again if "Remember" was left
  unchecked. Flagging this as a reasonable-but-imperfect interpretation for review — a true ephemeral
  channel is future CPE-1499 work.

- **`src/App.svelte`** — loads `connections` (`commands.connectionsList()`) and now always loads `shared`
  (`loadShared()`, previously pull-only on the Home Shared tab) at startup, both **fire-and-forget** (not
  awaited inline) so they don't delay `restoreLastSession()` later in the same `onMount` chain — an
  awaited call there caused a real regression, see Verify below. New handlers: `onNetworkConnect` (checks
  `secretAlwaysRequired`, opens the secret prompt or connects directly), `connectNetworkConnection`
  (navigates via the existing `navigate`/`navigateB`, reading the SAME `error`/`errorB` the pane already
  surfaces to set the state dot — no duplicate `list_dir` call), `submitNetworkSecret`,
  `saveNetworkConnection`, `forgetNetworkConnection` (also calls `connection_secret_delete`),
  `disconnectNetworkConnection` (client-side state reset only — **there is no backend command to tear
  down a pooled remote session yet**; a natural CPE-1499 follow-up once a real session-status query
  exists). Renders the three new popovers at the App level, same convention as `AgentMenu`/`TagMenu`/
  `SmartFolderMenu`.

- **Docs (CPE-579):** `src/docs/31-network.md` + a `network` entry in `src/lib/sectionDocs.ts` (`Section`
  type + `SECTION_DOC` map), following the vaults/terminal/file-health precedent (a cross-cutting feature
  doc, not wired into `currentSection()`/`DOC_SECTIONS` since it isn't a full-screen view). Guard test
  `sectionDocs.test.ts` passes.

- **`gui-smoke/specs/network.smoke.ts`** (new, scaffold only, NOT run as part of this slice — no
  display/WDIO harness available here): asserts the always-visible "Network…" entry point renders on a
  fresh app process (guaranteed empty-state, mirroring `instant-search.smoke.ts`'s reasoning) and that
  clicking it opens the add-connection popover with its expected fields (sftp/webdav, Password/Key file),
  then Escape closes it. Does **not** drive a real remote connect (no live SFTP/WebDAV server in the
  harness) or exercise the row context menu/status dots — those need either a live test server or the
  Visual Critic's judgment.

**Unit-verified:**
- `npm run check` — 0 errors, 0 warnings.
- `npm test` (vitest) — **232 files / 2582 tests, all green**, including the new `network.test.ts` (27
  tests) and the existing `sectionDocs.test.ts` guard.
- `cargo build --lib`, `cargo clippy --all-targets -D warnings` (both default and `sidecar-platform`
  feature sets) — clean.

**A real regression caught + fixed during Verify:** the first pass awaited `commands.connectionsList()`
inline in `onMount`, which delayed `restoreLastSession()` (called later in the same sequential chain) just
enough that ~60 existing App-mount tests started racing it — a test's manual `navigate()` would land, then
the delayed `restoreLastSession()` → `loadPath(HOME)` would fire afterward and wipe the listing back to
empty. Fixed by making both the connections load and (already-fire-and-forget) `loadShared()` non-blocking
so they can't delay session restore — full suite green after.

**VISUAL / interaction sign-off is OUTSTANDING** — nothing in this Work Log claims the UI has been looked
at. What a reviewer/Visual Critic/the user should specifically check:
1. The Explore section's new "Network…" row (icon + spacing consistent with Repositories/Agent Board/
   Workbench above it).
2. The add-connection popover's layout (protocol dropdown, half-width User/Port fields, the auth radio
   choice, the key-file Browse row) — border visibility, spacing, and the error message's placement on an
   invalid submit.
3. Once a connection is saved: the collapsible Network section's header (icon, "+" hover reveal), the
   status dot's three colours or its right-click menu (Connect/Disconnect/Edit/Forget), and the secret
   prompt popover.
4. Whether the judgment call above (the always-visible "Network…" row vs. a fully-hidden section) reads
   right, or whether the Reviewer prefers a different first-connection entry point.
5. Whether the OS-discovered-share tier 2 rows (icon reused from Drives, no context menu in this slice)
   look right sitting under saved connections.

None of the above have been screenshotted or run through gui-smoke; this ticket stays in `Doing` pending
that pass.
