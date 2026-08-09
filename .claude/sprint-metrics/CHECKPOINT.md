# Sprint Checkpoint

## RUN 2026-08-09 (CLI, "keep working on the sprints" — user away w/ intermittent requests) — batch 21 WRAPPED
**State:** `main` @ origin `aaa64fff` (clean, CI GREEN, 0 worktrees, Backlog has only queued/user-gated work).
Lock released, wakeup cancelled. Budget ~135/200 (well under the 150 reset line — stopped on *work-ran-dry*,
not budget: the remaining ready headless work is thin, the rest is user-gated).

### Shipped this batch (5 merges, all gauntlet-verified Reviewer+UAT, 0 escaped defects after the one caught below)
User asked (while mostly away) to build out the **Network / left-pane UX** and a **Gource** showcase. Delivered:
- **CPE-1516 (#736)** — Network is now a **permanent top-level sidebar section** (was hidden until a
  connection existed; entry was buried in Explore). Frontend.
- **CPE-1519 backend (#737)** — **Windows-native network discovery** via `WNetOpenEnum`/`WNetEnumResource`
  (`discover_network_windows`, `#[cfg(windows)]`, async+spawn_blocking+6s-bounded, depth/buffer caps); pure
  map/flatten/dedup in `cpe_server::net_share`. Opus-reviewed (unsafe FFI). No new dep (windows crate feature).
- **CPE-1520 (#739)** — **user-reorderable sidebar sections** (drag headers + persisted `sidebarOrder` store +
  reset; CSS-order on flex children, low-churn). Frontend.
- **CPE-1519 frontend (#741)** — **"Discovered on your network" tier** in the Network section: calls the
  command (raw invoke, it's excluded from typed bindings), dedupes across all 3 tiers, one-click pre-filled
  add (`smb` scheme; connect fails cleanly until CPE-1504). Feature now COMPLETE E2E.
- **bindings base-fix (#740)** — see incident below.
Also: **Gource visualization → PR #738** (CPE-1522, a SEPARATE user request) — a weekly Actions render of the
repo history embedded under the README hero, published to an orphan `gource` branch (NOT committed to main —
would bloat history). **Open for the USER to review/merge** (outward-facing landing page). Filed **CPE-1521**
(WNet outer-loop entry-cap hardening, opus-review follow-up to #737).

### Incident caught + fixed (lesson)
#737 changed `NetShare.kind`'s **doc comment** (a `specta::Type`) but didn't regenerate `bindings.gen.ts` → the
CI **Typed-bindings drift guard** reddened `main` (Backend ubuntu). The Windows-local gauntlet + the drift
*unit test* both passed, masking it; only a later PR's inherited-red surfaced it. Fixed by regenerating +
merging #740; main green again (confirmed `completed/success`). **Lesson (saved to memory
[[regen-specta-bindings-on-struct-change]]):** for ANY backend/specta PR, regenerate bindings on a doc-comment
change too, and **verify the ubuntu Backend drift-guard CI leg before merging** — don't merge on the
Windows-local gauntlet alone. `cargo` is NOT on the non-interactive shell PATH → use
`%USERPROFILE%\.cargo\bin\cargo.exe` (PowerShell).

## To resume (say "resume the sprint" in a fresh session)
The clean-headless well is thin again. Remaining queue (priority order):
1. **CPE-1521** — WNet outer-loop entry-cap hardening (backend, small, `#[cfg(windows)]` — buildable+testable
   on THIS Windows machine; low priority, bounded risk). The one genuinely-ready headless ticket.
2. **CPE-1518** — E2E-verify shipped SFTP/WebDAV/FTP + the new WNet discovery against the real **QNAP TS-133**
   NAS. **ATTENDED — needs the hardware (arrives/installed 2026-08-10)** + the user's LAN. See
   [[qnap-nas-test-target]].
3. Epics needing decomposition / user decisions: **CPE-1504** (SMB — crate-risk, Windows-UNC leg testable vs
   QNAP now), **CPE-1500** (OS-mount), **CPE-1517** (LAN mDNS/SSDP discovery — the cross-platform complement to
   the Windows-native tier just shipped; needs a dep decision).

**Owed to the USER (async, non-blocking):**
- Review/merge **Gource PR #738** (+ then run the workflow once to populate the `gource` branch).
- **Visual sign-off** on the sidebar changes (permanent Network section + drag-reorder + Discovered tier) —
  none are pixel-verified (jsdom only). A `build→install→run` or a gui-smoke Visual Critic pass would close it.
- **QNAP E2E** (CPE-1518) once the NAS is set up.

**Tuned defaults (seed next run):** frontend tickets = sonnet, 2-wide, ~20-35m each, clean; unsafe-FFI diffs =
OPUS adversarial reviewer (caught the null-PWSTR question on #737, confirmed guarded). Foreman-apply worked
well for the mechanical bindings regen (0 agents). Z: drive I/O-saturates under concurrent cargo builds — cap
heavy Rust builds low; GitHub API intermittently slow — run merges/git in background, Read `.output` files
instead of shell `cat`.
