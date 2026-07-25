---
id: CPE-411
title: AI Console — revert garbling live-update; badge right + Model fills; bottom status bar + grip
type: bug
priority: high
estimate: S
status: Done
created: 2026-07-14
closed: 2026-07-14
tags: [ui, ai-console, bug]
---

## Fixes
- REVERT CPE-410 live-update: the poll appended alt-screen-stripped delta bytes, so an ANSI escape
  straddling a fetch boundary misaligned the offsets and corrupted the buffer — garbled when
  scrolled. Back to the solid write-once-on-open behaviour (keeps CPE-408 width + CPE-409 fresh).
- Top row: moved the install badge to the far right so the Model field (.grow) fills the space.
- Added a bottom status bar (project folder + selected agent/provider/model) with a sizing grip
  (Tauri resize when available; the window's native edges resize it regardless).

- [x] Session output no longer garbles when scrolled
- [x] Install badge far right; Model fills the extra space
- [x] Bottom status bar + grip
