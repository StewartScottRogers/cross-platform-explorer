---
id: CPE-1163
title: "Populate the Home \"Shared\" tab with network / mapped / SMB shares + its context menu"
type: feature
component: Backend
priority: medium
status: Backlog
tags: ready
created: 2026-07-31
---

## Summary
User-decided (2026-07-31): the empty/disabled **Shared** tab on Home should list **network locations** —
mapped drives, `\\server\share` UNC paths, and (stretch) discovered SMB shares — and its rows get a
right-click menu like the other Home lists (CPE-1162). Split out from CPE-1162 because it needs real,
platform-specific backend enumeration, and it should build on CPE-1162's `homeItemContext` machinery.

## Scope
### Data (the new part — backend, platform-aware)
- **List mapped/network drives** the OS already knows about:
  - Windows: currently-mapped network drives + UNC connections (e.g. via `net use` / WNet `WNetOpenEnum`
    APIs, or reading mapped-drive letters whose type is network). This is the MVP.
  - macOS/Linux: mounted network filesystems (SMB/NFS) from the mount table. Keep it best-effort +
    cross-platform-graceful (empty list is fine where unsupported).
- **Add a network location** (a small "＋ Add network location" affordance): user types a `\\server\share`
  (or `smb://`) path; it's remembered and listed. (MVP can be just this + the enumerated mapped drives.)
- Persist the user-added locations (settings), like recents/favorites.
- **Full SMB/network discovery** (browsing the LAN for shares) is a STRETCH — heavier + flaky across
  platforms; scope the first pass to already-mapped drives + manually-added locations, note discovery as a
  follow-up.

### Menu (reuse CPE-1162)
Once Shared has rows, wire the same `homeItemContext` machinery so a Shared row's right-click offers:
**Open** (navigate into the share), **Copy path** (the UNC/mount path), **Disconnect / Unmap** (for a mapped
drive; or "Remove" for a user-added location), **Properties**. Adapt labels to the network peculiarity
(Disconnect vs. delete; a share isn't a normal local folder). Handle **unreachable** shares gracefully
(offline server → Open shows a clear error, Remove/Disconnect still works).

## Acceptance Criteria
- [x] The Shared tab is enabled and lists mapped/network drives (Windows MVP) + any user-added network
      locations; empty state reads sensibly where unsupported.
- [x] "Add network location" lets the user add a `\\server\share` / `smb://` path; it persists + appears.
- [x] Right-clicking a Shared row (reusing CPE-1162's machinery) offers Open / Copy path / Disconnect-or-Remove
      / Properties, adapted to the network context; unreachable shares degrade gracefully.
- [x] Backend enumeration is platform-aware + best-effort (no crash / no hang on a dead server — time-bounded);
      `npm run check` + `cargo clippy --all-targets --features sidecar-platform -- -D warnings` clean (both
      modes if a command is added); bindings regenerated if a specta command is added; tests cover the
      enumeration + the menu dispatch.

## Notes / dependencies
- **Depends on CPE-1162** (the `homeItemContext` menu machinery) — build after it lands.
- Enumeration is the real work; keep it time-bounded (a dead network mount must never hang the Home view —
  async + spawn_blocking per the async-commands convention).
- Cross-platform: Windows mapped drives first; macOS/Linux mounts best-effort; LAN discovery deferred.

## Work Log

**2026-07-31 — implemented (MVP), branch `cpe-1163-shared-network-shares`.**

Built on CPE-1162's view-agnostic `homeItemContext` machinery by adding a fourth `view: "shared"` value
end-to-end (HomeView → ExplorerPane → App → ContextMenu). The Shared pill is now enabled and loads
**pull-only** — when the tab is opened (or restored on mount), never on a timer — so an offline server
can't slow the rest of Home.

**Enumeration approach (per OS), all time-bounded + best-effort:**
- **Windows (primary):** parse `net use` output. `net use` reads the *local* redirector table (it does
  not contact the servers, so it returns even when a mapped server is offline), and it's run through a
  bounded subprocess helper (`run_bounded_capture`, 3 s cap, kills + abandons on timeout, `CREATE_NO_WINDOW`).
  The parser scans each line for a **drive-letter token** (`Z:`) + a **UNC token** (`\\host\share`) —
  language-independent, so it's robust to localized status words and column widths.
- **Linux:** read `/proc/mounts` (a synthetic kernel file, non-blocking) and keep only network fstypes
  (cifs/smbfs/smb3/nfs/nfs4/fuse.smbfs/…); octal `\040` escapes in paths are decoded.
- **macOS:** run `/sbin/mount` (bounded) and parse `//host/share on /Volumes/x (smbfs, …)`, keeping
  network fstypes. Empty is a valid result everywhere unsupported.
- **User-added locations:** persisted in the frontend settings (`cpe.networkLocations`, mirroring
  favorites/pins) and merged into the backend result. The command signature is
  `list_network_shares(user_added: Vec<String>) -> Vec<NetShare>`; the backend validates each address via
  the existing `net_share::parse_share`, dedupes against the enumerated list (case/slash-insensitive), and
  returns the combined `{name, path, kind}` list. On a transport error the UI degrades to just the
  user-added rows.

**Domain logic lives in `cpe-server`** (`net_share.rs`: `NetShare`, `parse_net_use`, `parse_proc_mounts`,
`parse_macos_mount`, `user_share`, `combine_shares`), keeping lib.rs thin (a `spawn_blocking` dispatcher).
Windows-only OS code is `#[cfg(windows)]`'d with graceful empty/error fallbacks elsewhere so it compiles
clean on the CI 3-OS matrix.

**Commands + bindings:** two new specta commands — `list_network_shares` and `disconnect_network_share`
(Windows `net use <drive> /delete /y`, bounded; a clear error on other platforms / non-drive paths) —
registered in **both** `generate_handler!` and `collect_commands!`; `bindings.gen.ts` regenerated (adds
`listNetworkShares`, `disconnectNetworkShare`, `NetShare`) and confirmed drift-free on a fresh re-run.

**Menu (reuses CPE-1162):** a Shared row's right-click offers **Open** (navigate), **Open in new tab**,
**Copy path**, **Properties**, then a kind-specific **Disconnect** (mapped drive) or **Remove** (user
location); OS mounts get neither destructive action. User-added rows also show an inline ✕. Unreachable
shares degrade gracefully — the stat-based stale check is deliberately **skipped** for shares (statting a
dead server could stall), so Open surfaces its own error while Disconnect/Remove stay live. Blank Home
stays menu-less (CPE-1158/1162 unregressed).

**MVP vs deferred:** MVP = already-mapped drives / mounts + manually-added locations. **LAN discovery
(browsing the network for shares) is deferred** as noted in the ticket — heavier + flaky cross-platform.

**Verification:** `npm run check` 0/0; `cargo clippy --all-targets -D warnings` clean in **both** default
and `--features sidecar-platform` modes AND at the `crates/server` crate level (CI lints it directly);
`cargo test` green for the enumeration domain logic (22 net_share tests incl. `net use` parse, `/proc/mounts`,
macOS mount, user-share validation, combine/dedupe, empty-not-hang) + the `list_network_shares` command
test (user location surfaces even with no OS mounts; no hang); `npx vitest run` 1469 passed incl. new
HomeView Shared-tab, settings network-location, and ContextMenu shared-menu tests. In-app docs updated
(`src/docs/03-explorer.md` → Home screen → Shared).
