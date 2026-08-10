# Sprint Checkpoint

## RUN 2026-08-09 (CLI, BATCHED "run many sprints in batches") — batch 23 done, budget hand-off (NOT a stop)
**State:** `main` @ origin `317f23f3` (clean, 0 worktrees; #743 merged with all Backend/Frontend CI incl the
ubuntu drift-guard green). Lock released, wakeup cancelled. **Batched run 23/40 — CONTINUES; do NOT delete
BATCH-COUNTER.** Same-session sub-agent budget ~145/200 spent → hand off. **Resume in a genuinely FRESH session:
say "run many sprints in batches" (or "resume the sprint") to continue the 23/40 run with full budget.**

### Shipped this batch (23)
- **CPE-1523 (#743)** — **cross-platform mDNS/DNS-SD LAN discovery** (slice 1 of epic CPE-1517). New crate
  `crates/mdns` (`cpe-mdns`, dep `mdns-sd` 0.20.3), pure `map_mdns_service` (6-scheme table) + bounded
  `discover()`, `discover_network_mdns` command (cross-platform, IN specta bindings), frontend `mergeDiscovered`
  runs WNet+mDNS in parallel and dedupes into the existing Discovered tier (Sidebar.svelte unchanged). Opus dep
  audit + enum-blast-radius clean; UAT + CI green; bindings + both Cargo.lock done right (drift-guard passed).
  Live-LAN resolve owed (QNAP, CPE-1518). Filed follow-up **CPE-1524** (gate ＋Add on unsavable discovered
  schemes — the nfs:// nit).

### NEXT — ready buildable work for the fresh session (well is NOT dry — it's budget-limited)
Priority order (all frontend, headless-buildable, visual sign-off owed):
1. **CPE-1483** — Linux: Home landing doesn't render `/` root as a drive tile (bug; frontend/GUI).
2. **CPE-1508** — Image compare view (side-by-side / onion-skin / pixel-diff heatmap). Meaty frontend.
3. **CPE-1509** — File split/join dialog + context-menu entries (consumes the CPE-1491 backend).
4. **CPE-1524** — small: gate ＋Add on discovered rows whose scheme isn't savable yet (nfs). Good quick batch.
Then **CPE-1518** (QNAP E2E — ATTENDED, needs the NAS from 2026-08-10; now covers SFTP/WebDAV/FTP + WNet AND
mDNS discovery). Epics for later: CPE-1504 (SMB), CPE-1500 (OS-mount), CPE-1517 v2/v3 (SSDP/UPnP).

### Owed to the USER (async, non-blocking)
- Review/merge **Gource PR #738** (then run the workflow once).
- **Visual sign-off** on the sidebar: permanent Network section (CPE-1516) + drag-reorder (CPE-1520) +
  Discovered tier now cross-platform (CPE-1519fe + CPE-1523). jsdom-tested only.
- **QNAP E2E** (CPE-1518) once the NAS is up — real-hardware test of the whole Network feature incl. both
  discovery paths.

### Tuned defaults / lessons (seed next run)
frontend=sonnet 2-wide ~20-35m (a new-crate+dep+bindings ticket like CPE-1523 took ~1h — budget one heavy
ticket accordingly); new-dep/enum-change/specta diffs = OPUS adversarial reviewer (dep audit + enum
blast-radius). **The batch-21 bindings-drift trap did NOT recur** — the CPE-1523 worker regenerated
`bindings.gen.ts` + both Cargo.lock and verified `git diff --exit-code` clean before pushing; the ubuntu
Backend drift-guard CI leg was confirmed green before merge. Keep briefing that explicitly for any specta PR
([[regen-specta-bindings-on-struct-change]]). `cargo` NOT on non-interactive shell PATH →
`%USERPROFILE%\.cargo\bin\cargo.exe` (workers have it). Z: I/O-saturates under concurrent cargo builds; GitHub
API slow → background git/gh + Read `.output` files.
