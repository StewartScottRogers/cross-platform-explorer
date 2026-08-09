# Sprint Checkpoint

## RUN 2026-08-09 (CLI) — batch 22 WRAPPED (budget hand-off, NOT a stop — resume in a fresh session)
**State:** `main` @ origin `35ec186d` (clean, backend CI green, 0 worktrees). Lock released, wakeup cancelled.
Budget ~140/200 in THIS session (the user resumed in-session, so the cap did NOT reset) — stopping at the reset
line because the next ready ticket (CPE-1523) is a new-crate + new-dep + bindings job that needs full budget
headroom. **Resume in a genuinely fresh session (or raise `CLAUDE_CODE_MAX_SUBAGENTS_PER_SESSION`).**

### Shipped/done this batch (22)
- **CPE-1521 (#742)** — WNet discovery outer-pagination loop hardened (`WNET_MAX_TOTAL_ENTRIES=4096` cumulative
  + iteration cap, partial-results semantics) — closes the opus-review follow-up from CPE-1519. Merged, backend
  CI green. Reviewer+UAT passed.
- **Epic CPE-1517 (LAN discovery) ACTIVATED + decomposed.** Dep decided: **`mdns-sd`** (pure-Rust, no native
  Bonjour SDK, permissive, maintained) — see research [[mdns-discovery-dependency-2026-08-09]]. Windows-native
  leg already shipped (CPE-1519).

### NEXT TICKET — ready to build first in the fresh session
**CPE-1523** (`Ticketing/Tickets/Backlog/CPE-1523_mdns-discovery-slice1.md`) — mDNS slice 1, FULLY SPECIFIED:
- New crate **`crates/mdns` (`cpe-mdns`)** (path-dep on cpe-server + `mdns-sd`), mirroring cpe-ftp/webdav/sftp.
  Pure `map_mdns_service(...)→Option<NetShare>` (table: _smb→smb:445, _sftp-ssh→sftp:22, _webdav→webdav:80,
  _webdavs→davs:443, _ftp→ftp:21, _nfs→nfs:2049; else None) + impure `discover(timeout)→Vec<NetShare>`
  (browse 6 types, ~6s bound, dedup via `net_share::dedup_key`→make pub).
- Command **`discover_network_mdns`** (cross-platform, NOT cfg-gated, INCLUDED in specta bindings — unlike the
  per-OS `discover_network_windows`). `App.svelte loadDiscovered()`: run WNet + mDNS in parallel, merge+dedupe
  (pure TS helper, unit-tested). Tier-3 UI already renders it → no Sidebar change.
- **Chores (don't skip):** regen `bindings.gen.ts` (new specta command), regen root + src-tauri `Cargo.lock`
  ([[multiple-independent-cargo-locks]]), add the workspace member. **⚠ VERIFY the ubuntu Backend drift-guard
  CI leg is GREEN before merging** (batch-21 bit us here).
- **Folded fixes:** extend `discoveredShareToFormInput` for `scheme://host[:port]` paths; add `ftp` to
  `SUPPORTED_SCHEMES` (cpe-ftp ships).
- Note: touches `src-tauri/src/lib.rs` — serialize vs anything else editing that file.

### Rest of the queue (priority order)
1. CPE-1523 (above) — the one ready buildable ticket.
2. **CPE-1518** — QNAP TS-133 E2E (ATTENDED, needs the NAS — from 2026-08-10 — + user LAN). Now covers BOTH the
   WNet discovery AND the mDNS discovery + shipped SFTP/WebDAV/FTP. See [[qnap-nas-test-target]].
3. Epics: CPE-1504 (SMB — crate-risk; Windows-UNC leg testable vs QNAP), CPE-1500 (OS-mount), CPE-1517 later
   slices (SSDP/UPnP v3).

### Owed to the USER (async, non-blocking)
- Review/merge **Gource PR #738** (then run the workflow once to populate the `gource` branch).
- **Visual sign-off** on the sidebar changes (permanent Network section CPE-1516 + drag-reorder CPE-1520 +
  Discovered tier CPE-1519fe) — jsdom-tested only, not pixel-verified.
- **QNAP E2E** (CPE-1518) once the NAS is up — the first real-hardware test of the whole Network feature.

### Tuned defaults / lessons (seed next run)
frontend=sonnet 2-wide ~20-35m; unsafe-FFI=opus reviewer (safe-Rust follow-ups=sonnet); Foreman-apply
mechanical fixes (bindings regen) = 0 agents. **Verify the ubuntu Backend drift-guard CI leg before merging ANY
specta/backend PR** — Windows-local gauntlet + the drift unit-test both pass while the CI shell-step fails
([[regen-specta-bindings-on-struct-change]]). `cargo` NOT on non-interactive shell PATH →
`%USERPROFILE%\.cargo\bin\cargo.exe` (workers have it on PATH; my own Bash/PowerShell tools don't). Z: drive
I/O-saturates under concurrent cargo builds; GitHub API intermittently slow → background git/gh + Read `.output`
files, don't hammer.
