---
id: CPE-483
title: "Clear orphaned session-daemons on startup/install so they can't lock the sidecar binary"
type: Defect
status: Done
priority: Medium
component: Backend
tags: [ready]
estimate: 1-2h
created: 2026-07-15
closed: 2026-07-16
epic: CPE-261
---

## Summary
Leftover `ai-console.exe --session-daemon` processes (from the CPE-309 daemon builds — they outlive
the app by design) caused two real failures: (1) they held `sidecars\ai-console.exe` **file-locked**,
so the NSIS installer silently skipped updating the sidecar, leaving a **new host running a stale
sidecar** (the black-terminal saga, see [[CPE-309]]); (2) a surviving daemon kept serving old,
output-less sessions. The app must never let an orphaned daemon linger.

## Acceptance Criteria
- [x] On host startup, detect + terminate any orphaned `ai-console --session-daemon` process that this
      host does not own, and delete a stale daemon port file. Scope it tightly (only this app's sidecar
      binary path) so it never touches unrelated processes.
- [x] The `/run` install flow and `/remove` uninstall flow kill **all** `cross-platform-explorer` +
      `ai-console` processes (including `--session-daemon`) **before** running the installer, so the
      sidecar binary is never locked during an update. (Docs updated in this ticket.)
- [x] If/when the daemon path is re-enabled (CPE-309), the host owns + reaps it deterministically on
      exit; a graceful shutdown leaves no orphan.
- [x] A note in RELEASING/install docs: "a stale sidecar shows as the host version in the registry —
      verify `sidecars\ai-console.exe` timestamp matches the host exe after install."

## Notes
Discovered while root-causing the AI Console black terminal (CPE-309, 2026-07-15). The immediate
lesson (kill-all-before-install) has been folded into the `/run` + `/remove` command docs; the
host-startup cleanup is the remaining code slice here.

## Work Log
2026-07-16 — Picked up. Estimate 1-2h. Plan: add a host-startup orphan-daemon sweep in the
`sidecar-host` crate (it already carries `sysinfo`), wire it into the Tauri `run()` setup hook behind
the `sidecar-platform` feature, then finish the `/remove` kill-all doc and the RELEASING stale-sidecar
note (the `/run` side was already done).
2026-07-16 — AC1: built `sidecar/host/src/reaper.rs`. Pure, unit-tested predicate
`is_our_session_daemon(exe, cmd, our_exes)` — true only when the cmdline carries `--session-daemon`
AND the process exe path-matches one of *our* sidecar binaries (canonicalize with a case/separator-
insensitive fallback for Windows). `reap_orphan_session_daemons()` enumerates via `sysinfo`, kills
matches, and deletes the stale `%TEMP%\cpe-ai-console\session-daemon.port`. Wired into `run()` via a
`sidecar-platform`-gated `.setup()` that runs **before** any daemon is spawned — so every match is by
construction not host-owned. 6 reaper unit tests pass; `cargo check`/`clippy --all-targets -D warnings`
clean in both feature modes.
2026-07-16 — AC2: `/run` already killed-all + verified sidecar timestamp (run.md, prior work).
Updated `/remove` Step 2 to kill **all** `cross-platform-explorer` + `ai-console` (incl.
`--session-daemon`) and clear `%TEMP%\cpe-ai-console` before uninstalling, so the sidecar binary is
never locked during removal.
2026-07-16 — AC3: verified the forward-looking guarantee already holds — `impl Drop for
SessionDaemonHandle` (ai-console `session_supervisor.rs`) reaps **only** a daemon it spawned
(`child: Some`), leaving a discovered/host-owned one alone. With the startup sweep as belt-and-
suspenders, re-enabling the daemon path (CPE-309) yields deterministic own+reap with no orphan.
2026-07-16 — AC4: added a "Verify the sidecar actually updated" section to RELEASING.md — the
registry/app version reflects the host exe, not the bundled sidecar; check that
`sidecars\ai-console.exe` LastWriteTime matches the host exe, and how to recover.

## Resolution
The app now self-heals against orphaned session-daemons, and the install/uninstall flows can no
longer leave the sidecar binary locked.

**Code (AC1 + AC3)** — new `sidecar/host/src/reaper.rs`:
- `is_our_session_daemon(proc_exe, proc_cmd, our_exes)` — the pure, unit-tested match rule: the
  process must be a `--session-daemon` invocation *and* its executable must path-match one of our
  bundled sidecar binaries. Path comparison canonicalizes, with a Windows-friendly
  case/separator-insensitive fallback.
- `reap_orphan_session_daemons(our_exes, port_file)` — enumerates processes via `sysinfo` (already a
  host dep), terminates matches, and removes the stale port file. Best-effort, never fatal.
- `default_session_daemon_port_file()` — mirrors ai-console's `session_supervisor::default_port_file()`
  (`%TEMP%\cpe-ai-console\session-daemon.port`); the host can't depend on the sidecar crate.

Wired into `src-tauri/src/lib.rs`: `sidecar_ai_console_exes()` resolves this app's bundled + config
sidecar binary paths, and a `sidecar-platform`-gated `.setup()` hook runs
`reap_orphan_session_daemons_on_startup()` on launch — before the host spawns any daemon, so every
match is one it does not own. AC3's re-enable guarantee is met by the pre-existing
`Drop for SessionDaemonHandle` (reaps only a self-spawned daemon), now backstopped by the sweep.

**Docs (AC2 + AC4)**: `/remove` (`.claude/commands/remove.md`) Step 2 now kills **all**
`cross-platform-explorer` + `ai-console` processes (incl. `--session-daemon`) and clears the temp dir
before uninstalling; `/run` already did the equivalent. `RELEASING.md` gains a "Verify the sidecar
actually updated" section (version reflects the host, not the sidecar — check the timestamps).

**Verification**: 6 new reaper unit tests pass; `cargo check` + `cargo clippy --all-targets
-D warnings` clean with and without `sidecar-platform`.

**Tradeoffs**: the sweep is scoped strictly to *our* binary path so it never touches an unrelated
`ai-console` (a dev build, a second install). It relies on `sysinfo` for exe path + cmdline, which
lives only in the `sidecar-host` crate — so the plain explorer (feature off) pulls none of this.

**Files**: `sidecar/host/src/reaper.rs` (new), `sidecar/host/src/lib.rs`, `src-tauri/src/lib.rs`,
`.claude/commands/remove.md`, `RELEASING.md`.
