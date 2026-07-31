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
- [ ] The Shared tab is enabled and lists mapped/network drives (Windows MVP) + any user-added network
      locations; empty state reads sensibly where unsupported.
- [ ] "Add network location" lets the user add a `\\server\share` / `smb://` path; it persists + appears.
- [ ] Right-clicking a Shared row (reusing CPE-1162's machinery) offers Open / Copy path / Disconnect-or-Remove
      / Properties, adapted to the network context; unreachable shares degrade gracefully.
- [ ] Backend enumeration is platform-aware + best-effort (no crash / no hang on a dead server — time-bounded);
      `npm run check` + `cargo clippy --all-targets --features sidecar-platform -- -D warnings` clean (both
      modes if a command is added); bindings regenerated if a specta command is added; tests cover the
      enumeration + the menu dispatch.

## Notes / dependencies
- **Depends on CPE-1162** (the `homeItemContext` menu machinery) — build after it lands.
- Enumeration is the real work; keep it time-bounded (a dead network mount must never hang the Home view —
  async + spawn_blocking per the async-commands convention).
- Cross-platform: Windows mapped drives first; macOS/Linux mounts best-effort; LAN discovery deferred.
