---
id: CPE-331
title: "Robust uninstall: native binary + running-session guard"
type: Feature
status: Open
priority: Low
component: Backend
created: 2026-07-13
---

## Summary

Reference uninstallers (a) refuse if a live agent session holds a file lock (e.g. Claude
sets CLAUDECODE=1) to avoid a half-removed state, and (b) remove BOTH the npm global
package AND the native-installer binary (`~/.local/bin/claude.exe`), leaving user config
intact. Ours runs only `npm rm -g`, so a native-installed CLI or a locked binary is left
behind. Steal: detect a running session and bail with guidance; remove the native binary
too; report what was/wasn't removed.

## Acceptance
- Uninstall removes both install methods and refuses cleanly (with guidance) when the
  agent is running.
