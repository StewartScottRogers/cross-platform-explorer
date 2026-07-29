---
id: CPE-318
title: AI Console runtime assembly (spawn + mount)
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 2-3h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

Assemble the end-to-end AI Console UI mount from the pieces built in CPE-271/272/316/317:
the app spawns the AI Console sidecar, reads the UI URL it serves, and mounts it in an
iframe pane. Feature-gated and non-fatal, so the plain explorer is untouched.

## Acceptance Criteria

- [x] Feature-gated backend command `sidecar_start_ai_console` spawns the sidecar via the
      supervisor, handshakes (with the launch token), reads the `ui:<url>` Status, keeps
      the connection alive in managed state, and returns the URL — errors returned, never
      panics; binary resolved from env / resource dir / dev fallback.
- [x] Default build (feature off) unaffected — delete-test holds.
- [x] Frontend: "Open AI Console" in Settings → `startAiConsole()` → an overlay pane
      mounting `SidecarPane` at the URL; a clear notice if unavailable.
- [x] `cargo check` (default + `--features sidecar-platform`) + clippy, `npm run check`,
      vitest, and production build all pass.

## Resolution

Backend (feature-gated, `src-tauri/src/lib.rs`): `AiConsoleState` (managed
`Mutex<Option<ProcessConnection>>`), `resolve_ai_console_bin` (env `CPE_AICONSOLE_BIN` →
resource dir → dev target), and `sidecar_start_ai_console` which spawns → handshakes
(token-authenticated, grants the sidecar's 4 requested caps) → reads a bounded number of
frames for the `ui:<url>` announcement → stashes the connection alive → returns the URL.
`sidecar-contract` added as an optional dep alongside `sidecar-host` under the
`sidecar-platform` feature. Frontend: `startAiConsole()` client + an "Open AI Console"
button in Settings + an overlay in App mounting `SidecarPane`. No CSP change needed
(app CSP is `null`).

**Verified headlessly:** default + feature `cargo check`/clippy, `npm run check` 0/0,
vitest 264, production build. The individual links were already proven — ai-console serves
+ announces its UI over a real process (CPE-271), and supervisor spawn/handshake/recv over
real processes (CPE-265 E2E). **Pending your eyes:** building the app with the feature,
clicking Open AI Console, and confirming the iframe renders — the one step that needs the
running GUI.

## Work Log
2026-07-13 — Wired the runtime assembly during dayshift (CSP turned out to be null, so no
security-config change was needed). Backend + frontend compile/build green; GUI render is
the remaining with-user check.
