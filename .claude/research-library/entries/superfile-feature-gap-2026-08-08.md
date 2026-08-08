---
slug: superfile-feature-gap-2026-08-08
title: superfile (TUI file manager) feature-map vs CPE — gap analysis for PM epic ideation
tags: [product, epics, superfile, feature-parity, pm-reference]
status: current
created: 2026-08-08
sources:
  - https://github.com/yorukot/superfile
  - https://superfile.dev/overview/
  - https://superfile.dev/changelog/
  - https://superfile.dev/configure/custom-hotkeys/
  - https://superfile.dev/getting-started/image-preview/
---
## Bottom line
**CPE already matches or exceeds superfile on nearly every axis.** superfile is explicitly a lightweight,
UI-focused TUI file manager (its own docs recommend Yazi for "full-featured"). CPE's shipped/planned epics
(dual-pane CPE-617, global instant search CPE-703, Spotlight frecency CPE-704, metadata columns CPE-707,
Terminal Dock CPE-714, treemap disk-usage, transfer queue CPE-613, thumbnails CPE-718, trash routing) meet or
beat superfile's equivalents — several by a wide margin (frecency Spotlight > zoxide dirs-only; treemap >
plain disk list; sha256+bitrot > MD5 plugin; rich metadata columns > exiftool plugin).

## Genuine gaps (→ epics filed 2026-08-08)
1. **Hotkey customization / remap** — superfile has `hotkeys.toml`; CPE has ZERO rebinding (all hardcoded).
   Highest-impact gap, cleanly additive, frontend-only. → **EPIC CPE-1484** (Proposed).
2. **Binary architecture detection (ELF/PE/Mach-O CPU arch)** — superfile parses machine type; CPE detects
   format but not arch. Cheap, backend-only, headless. → **TICKET CPE-1485** (Backlog, epic CPE-1000).
3. **User theme/accent customization** — superfile has theme files; CPE is deliberately light-theme-only
   (single `:root`). CONTENTIOUS — contradicts a documented convention → PM/USER decision, NOT auto-filed.
4. **Browsable in-app Trash bin** — CPE trashes via the `trash` crate but can't browse/restore/empty in-app.
   Low value (OSes expose trash well). → **EPIC CPE-1486** (Proposed).

## NOT a fit / not recommended
- Terminal-graphics image preview, `cd_on_quit`, shell-specific verbs → moot for a GUI app.
- Third-party plugin *execution* marketplace → security-sensitive, off-purpose; superfile's "plugins" are
  really first-party opt-in modules, which CPE already ships as first-class no-dep features.

## Housekeeping flagged
CPE-705 archive suite engine is Done but context-menu wiring + password-prompt UI unfinished (2026-07-30) —
close it out (superfile exposes compress/extract on two keystrokes).

## Reuse
Standing PM reference per [[superfile-pm-reference]] (memory). Re-check superfile releases periodically for new
features. Greenlit-first: CPE-1484 (hotkeys); bundle CPE-1485 (arch detection) when CPE-1000 is next touched;
hold CPE-1486 (trash) low; SURFACE theming to the user rather than build.
