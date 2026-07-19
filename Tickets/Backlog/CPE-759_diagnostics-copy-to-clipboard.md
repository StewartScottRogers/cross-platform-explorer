---
id: CPE-759
title: Diagnostics — copy the readout to the clipboard for easy paste-back
type: feature
component: Frontend
tags: ready
created: 2026-07-19
status: Open
priority: medium
estimate: 1h
---

## Summary
Extend Diagnostics mode (CPE-758): let the user **copy the current diagnostic readout to the clipboard**
as text, so they can paste it straight back into a chat/support message. Right now the on-screen overlay
is read-only (`pointer-events:none`) — you'd have to transcribe the numbers by hand.

## Scope
- A **"Copy" affordance** on the Diagnostics overlay (a small button — which means the overlay needs
  `pointer-events:auto` on that control while staying click-through elsewhere), AND/OR a keyboard shortcut,
  AND/OR an Application-menu / command-palette entry "Copy diagnostics".
- Format the copied text usefully and pasteably, e.g. a compact table:
  ```
  Diagnostics (2026-07-19 14:03) — 24 calls
   340ms  list_dir_stream
    15ms  forge_repo_status
     5ms  disk_space
  slowest: list_dir_stream 340ms
  ```
  Include the app version + OS so pasted reports are self-describing.
- Use the OS clipboard (the app already has a copy-as-path / clipboard path; reuse it — `writeText`).
- Consider copying the FULL recent-call buffer (`DIAG_CAP`), not just the visible window.

## Open questions (resolve at build)
- Trigger: overlay button vs. shortcut vs. menu item — pick the most discoverable (probably a button on the
  overlay header + a command-palette entry).
- Include a rolling per-command aggregate (count / total / max) in the copied text, or just the raw list?

## Acceptance
- [ ] One action copies the current diagnostics (recent OS calls + durations + slowest + app/OS header) to
  the clipboard as readable text.
- [ ] The overlay stays click-through except for the copy control.
- [ ] Pasted output is self-describing (version/OS/timestamp) and easy to read.

## Notes
Extends CPE-758 (Diagnostics mode). Requested so diagnostic output can be pasted back quickly. See
[[diagnostics-mode-instrument-os-calls]].
