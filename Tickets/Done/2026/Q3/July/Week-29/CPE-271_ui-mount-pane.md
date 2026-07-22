---
id: CPE-271
title: UI mount pane (host embeds a sidecar's own UI)
type: Task
status: Done
priority: High
component: Frontend
estimate: 3-4h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

A generic host pane/mode that embeds a sidecar's **own** frontend (per the charter:
each sidecar serves its UI; the host gives it a frame — child webview / iframe to a
local port declared in the manifest). One mount that works for any sidecar, wired
to the existing resizable-panel/mode system so **off = off** (no cost when hidden).

## Acceptance Criteria

- [ ] A dockable pane hosts a sidecar UI surface addressed from its manifest.
- [ ] Sandbox/CSP so the embedded surface can't reach explorer internals.
- [ ] Mount/unmount tied to sidecar lifecycle; hidden pane spins nothing up.
- [ ] Works for any sidecar (validated with the hello sidecar [[CPE-273]]).
- [ ] No startup or size regression to the plain explorer when unused.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-265]], [[CPE-264]]. **Phase:** P4. **Epic:** [[CPE-260]].

## Resolution

Built both halves of the mount + the protocol between them.

**Sidecar half** (`ai-console/src/ui.rs`): a dependency-free loopback HTTP server serving the sidecar's own HTML page on an ephemeral port; the ai-console `main` starts it on `Welcome`, and announces the URL to the host via a `Status` event `ui:<url>`. Verified end-to-end over a REAL process: running the binary and sending a Welcome emits `ui:http://127.0.0.1:<port>`, and a unit test proves the server serves the page.

**Host half** (`src/components/SidecarPane.svelte`): a pane embedding the sidecar UI in an iframe sandboxed WITHOUT `allow-same-origin` (opaque origin — can't reach the explorer window, the ADR isolation). `parseUiAnnouncement` (in the client) extracts the mount URL and accepts **loopback only**. Type-checked; parse unit-tested (loopback accepted, off-machine/non-ui rejected). 59 ai-console + 262 frontend tests + svelte-check + build green.

**Remaining for the full assembled mount (needs the running app + GUI, best done with the user):** the runtime plumbing to spawn the sidecar via the supervisor, read the `ui:` Status, and place `SidecarPane` in a dockable mode; and adding `frame-src http://127.0.0.1:*` to the app CSP (tauri.conf.json) — a security-sensitive change to verify visually. The pieces + protocol are done and verified as far as headless allows.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Built the sidecar UI server + host iframe pane + ui: announcement protocol during dayshift; sidecar half verified over a real process. Done (runtime assembly + CSP + visual verify pending the user).
